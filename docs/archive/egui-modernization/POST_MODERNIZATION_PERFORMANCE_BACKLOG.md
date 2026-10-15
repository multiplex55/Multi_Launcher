# Post-modernization performance backlog — future task

Authority: `PERFORMANCE_ACCEPTANCE_ADDENDUM.md`. This backlog is not a prerequisite for Track B or final implementation completion. Do not automatically launch it after theming. Raw historical evidence remains immutable.

## Comparison points and available evidence

| Point | Source / evidence | Limits |
|---|---|---|
| Original egui0.27.2 | Original baseline72A01BD5; corrected test-fixture/benchmark source `f45e377390284cc1ba6cf474fcd23e008514318e`. Original release executable990081; `release-small-control/receipts/original-release-small-002.exe`. Original CPU owner logs/indexes recorded in benchmark-results.md | Separate original profiles/builds and instrumented QA captures; do not mix them. Original26 vs migrated25 painted rows and old/new lifecycle owner boundaries are documented |
| Integrated migrated geometry | `79653eca51c80c8777b1b7e9ddaf6fd9a826e823`; release F26D (`release-full-root-size-only-001/receipts/current-release-multi_launcher-libtest.exe`), source binding/index/raw body runs under that directory | Latest integrated geometry comparison; later live GUI guards are not measured by this exact run |
| Chosen live source at policy change | `6527ddc15b6aa0c83fb2cbe41f5ceef9e4104669`; integrated measurement/cache plus closed-presentation guards | Actual live compilation/affected correctness passed at71a41051; subsequent functional stabilization accepted at migration-only af9d1183 with documented native QA gaps. Do not substitute isolated candidates |
| Migration-only checkpoint | `af9d11838db9448461be920790d926cb22b1cd40`; Glow/egui0.36.2, actual Cargo.lock18180D23D651F49D281E9C939DF60E082F9D157D336401CA255EEBA4187EC1C8; scoped correctness/format/Clippy logs under m2d/corrections-005/006 | No full benchmark executed for this exact source. Source796 geometry remains the available integrated reference, not native latency or exact af9d timings |
| Final themed source checkpoint | `adcda8dd3bfa49b5f016bd55c2e6202ad61974ad`; C24 plus M6 native frame/viewport corrections. Broad001 historical5251pass/12fail, all12 closed in final179/179 affected006; normal build001 passed. Current-source images/normal steady-state receipt follows in m6-final-acceptance.md | No full benchmark for this exact source. Full original/migrated/themed statistical campaign deferred, not completed |

Known accepted source796 release ROOT debt:10k cold p50 127.5695→166.2805ms, +38.7110ms/+30.345%; warm0.2003→0.3171ms, +0.1168ms/+58.313%. These are synthetic CPU-frame/geometry measurements, not native launch/hotkey timing or universal operation limits. Preserve all counts, tails/ranges, separate runs and fixed-order limitations in `benchmark-results.md` and `receipts/m2a-production-exact-measurement.md`.

## Deferred or retained work

Artifact paths below are relative to `target/egui-modernization/m2a/`.

| Work | Disposition | Preserved source/findings |
|---|---|---|
| Integrated shaping plan/raw-result reuse, paragraph capacity, exact measurement | Adopted already; retain under revised policy | Live vendor source and PATCH.md; production receipts and correctness contracts |
| Closed ThemeSettings/ClipboardModify/radial presentation guards | Integrated and retained; actual live M2-A correctness/check passed | Live6527/71a41051; `receipts/m2a-live-correctness-closeout.md`, `closed-aux-live-port-001/receipts`, `m2a-closed-aux-presentation.md`; archived candidate A438 is separate |
| Compact measurement | Deferred, not integrated | `compact-measurement-001/root`, C2E5 text_layout source; artifact6E7A84F5; six private contracts, ROOT and both Grid orders retained |
| Wider row-storage diagnostic and repair | Deferred, not integrated | `compact-storage-diagnostic-001/root` BBB42; `compact-full-row-storage-001/root` final0E8EAD63; repair artifact8C534F9C. Fixture outer storage192→96B, no timing causality claim. Four-body indexes BD2F24FB/C4854F21, combinedC0A0FE00; mixed warm/tail evidence retained |
| ASCII face lookup | Deferred, not integrated | `ascii-face-resolution-001`, artifact29F5D665; correctness passed, limited cold signal/mixed warm evidence |
| New ROOT warm thread-cycle attribution | Stopped before source preparation/build/body; deferred proposal only | `receipts/m2a-root-warm-cycles.md`; no QueryThreadCycleTime code or results exist |
| Glyph allocation prehash/IntMap | Rejected experimental route | Receipt `m2a-glyph-allocation-prehash-experiment.md` and ignored candidate; preserve negative findings |
| Other shaping/allocation/row/observer diagnostics | Historical adopted/rejected/unfinished status per existing receipt | `m2a-*` receipts and ignored directories; no new parity-only experiment during modernization |
| ETW/WPR/UAC or unchanged desktop-capture retries | Closed/deferred on current host | Prior failures/cancel retained; do not retry unchanged escalation/capture as a gate |
| Glow versus Wgpu competition | Deferred from M2-C | Existing source-backed packet retained historically; Glow stays explicit. No backend variant/comparison executed |

## Existing measurement infrastructure and identities

Use existing owners in `src/gui/render.rs`, `src/gui/notes_dialog.rs` and `src/performance/{track_c,track_c_workloads}.rs`. Existing Full recipe: five warmups/twenty samples, ROOT6/Grid30/Notes42/query3 cases; startup is separate. Existing controls include MULTI_LAUNCHER_PERF=1, ML_SKIP_CLIPBOARD_SYNC=1, ML_TRACK_C_BENCH_MODE=full, truthful source/profile labels. Reuse existing direct-libtest runners and parsers rather than introduce a new sampling framework.

Source796 frozen comparison manifests/lock differ in identity from the actual live checkout; use each artifact's preserved source-binding/input.json, not an assumed shared lock. Repair archives bind root manifest21B96CFD/lockFD610850, exact D75C/3AB2 serialized fixtures and source-qualified patches. Actual live lock at policy audit is18180D23D651F49D281E9C939DF60E082F9D157D336401CA255EEBA4187EC1C8. Record actual migration/final lock hashes when checkpoints are saved. Native/QA-instrumented binaries do not substitute for normal release metrics; fast-dev, iteration and release results stay separate.

## Existing vendor patch maintenance

| Patch | Purpose / classification | Coverage and removal consideration |
|---|---|---|
| eframe0.36.2 | Compatibility/correctness: hidden lifecycle/input/bootstrap hooks; immediate child screenshot routing | Focused lifecycle/hidden-input/viewport and screenshot dispatch tests; preserve until upstream equivalents cover app semantics; see vendor/eframe/PATCH.md |
| epaint0.36.2 | Performance: bounded shaping reuse, exact size-only measurement with shared geometry/atlas correctness | Existing key/bounds/Unicode/variable-font/full-output/size/atlas/cursor contracts; maintenance cost acknowledged, no broader fork for parity; see vendor/epaint/PATCH.md |
| egui0.36.2 | Integration/performance: shared WidgetText preparation and exact measurement | Focused formatting/wrapping/line-height/fallback/Galley pass-through contracts; remove with upstream equivalent preserving behavior |
| image0.25.6 | Correctness/compatibility: DDS capability reporting matches decoder | Capability/format integration tests; remove when upstream releases equivalent fix; image0.24 app decoder unchanged |
| egui-toast0.22.0 | Correctness/compatibility: hover pause under full-rect close interaction and opt-in bounded ROOT display below actual header | Stack/hover/TTL and complete400x220 warning/header/dismissal contracts; legacy show path retained. Remove when upstream covers egui0.36 interaction and equivalent bounded API; vendor/egui-toast/PATCH.md. No benchmark-parity optimization |

## Unmeasured native metrics and future sequence

Native hotkey-to-visible/display/GPU latency, true-idle CPU/GPU/memory at realistic workloads, broader physical chord/focus/placement/monitor/DPI/RDP coverage remain partly or wholly unmeasured. Keep available screenshots/source/automated evidence separate from actual desktop interaction. Record exact manual QA gaps during M2-B/M6-B; critical observed defects still need fixes.

Future priority: (1) measure representative native interaction/true idle on supported hardware using final sources; (2) reproduce practically important accepted debt with existing workloads and comparable profiles; (3) inspect concrete bottlenecks before low-complexity optimization, retaining all adverse data; (4) evaluate deferred candidates only if justified by absolute benefit/maintenance cost; (5) renderer comparison only if useful after functionality/theme delivery. Comprehensive three-way statistics and unavailable hardware coverage belong to that future task.

Final ROOT geometry diagnostics now key/report resolved selectable frame total margins instead of raw per-side button padding. ROOT list/grid intentionally retain inactive native frames; source policy differs from source796 and pre-M6 images. Compare source-qualified metrics/schema explicitly in future work; do not infer timing improvements from corrected2pt geometry or passing oracles. Observed1ppp containment is tested; fractional/mixed-DPI physical qualification remains missing.

Final bounded evidence completed at sourceadcda8dd: normal+QA builds0, four852x460 mode client captures saved/inspected, normal82.6401273s CPU0.140625s/RSS+4681728B/51threads/settings unchanged. Source/artifact/profile/fixture identities and limitations are in receipts/m6-final-acceptance.md and target/egui-modernization/m6/final-artifact-index-001.json. Desktop restore lookup AccessDenied; physical/native critical qualification, GPU/hidden-idle/long-run resources and full statistics remain future QA/measurement. No benchmark campaign or renderer competition executed at closeout.
