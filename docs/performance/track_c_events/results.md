# C3-C GUI event characterization — LOCAL ONLY

Status: C3-C PASS. Independent source, method, numerical, identity, privacy and lifecycle review CLEAR.22/29 checkpoints closed; C3-D/E decisions next.

Measured clean source: `c6c9d7edf16a8c6b77a44656ffee46275a08abfe`, fast-dev, Full 20,000 combined Actions, five warmups/twenty retained samples per recipe, zero exclusions. Small 200 uses the reviewed precommit source snapshot `785a956ffbcf79f4e32e318721494246723c0fa0dae4c9c59a9d0da0abc11249`; it is smoke evidence, not a G0 comparison. All source edits are cfg(test) characterization/lifecycle support. Production queue, handler, wake and drain policy are unchanged.

Nine unique ordinary tests passed: foundation eight plus the new frame contract. Two compile failures executed zero tests; the first new-contract execution failed because it expected disabled inline feedback. Those attempts and the corrected source-backed generation/selection oracle are retained in frame_verification.json. Four existing controls passed in that same attempt; only the new contract was retried and passed. Small original two owners/40 retained samples and companion one owner/three recipes/60 retained samples passed. Full original two owners/40 retained samples passed in103.548s, companion one owner/three recipes/60 retained samples in162.385s. These are test execution times, not compilation or per-sample latency.

## Actual CPU frame recipes

Every retained recipe completed one real root frame. Durations include trigger enqueue or start rendezvous, actual event drain, root frame and FullOutput disposal; posttimer semantic checks and teardown are excluded. Each row is a separate distribution of twenty whole-owner samples.

| Full recipe | Accepted/dequeued per owner | Owner p50 / p95 / max ms | Drain p50 / p95 / max ms | Pooled individual age p50 / p95 / max ms |
|---|---:|---:|---:|---:|
| Quiet settled | 0 / 0 | 0.3275 / 0.6325 / 0.7338 | 0.0010 / 0.0022 / 0.0022 | NOT APPLICABLE |
| Finite burst | 2 / 10 | 325.5210 / 346.4083 / 408.1639 | 324.2248 / 345.0458 / 406.8885 | 177.1163 / 246.7344 / 294.5901 (200 events) |
| Bounded producer | 18 / 18 | 555.7800 / 686.1509 / 916.5359 | 554.8840 / 685.0458 / 915.7127 | 224.6435 / 460.7048 / 658.1727 (360 events) |

The finite recipe retains eight setup-aged envelopes and adds two urgent triggers. Its setup-origin ages may exceed whole-owner elapsed. Separate untimed coordinator B completion adds one accepted/dequeued IndexReady, giving lifetime eleven; it is never folded into the ten-event measured owner. Quiet and bounded-producer recipes consume one initial Actions notification and run one actual settling root frame outside the owner. Finite performs zero setup root frames. Do not describe all three as identically font-warm.

Both Small and Full bounded-producer recipes had zero of twenty drain windows with acknowledged-send advancement: all sixteen producer sends were acknowledged before drain entry. The recipe name identifies its bounded producer arrangement; observed delivery is a finite burst. Ten of twenty Full whole-frame windows observed acknowledged-send advancement before draining, but all reducer windows were16→16. No sustained-arrival/starvation or native fairness conclusion follows. Handler/drain CPU work dominates the nonquiet owners, but a per-frame cap cannot shorten indivisible handler work. In the original Full controls, the largest FileReload handler per sample had median89.7784ms concurrent and87.0807ms finite; the sum of other FileReload calls had median353.7329ms and142.7658ms, respectively. Duplicate loading cost is real; no transport equivalence proof follows from it.

Whole-owner ViewportWake counts are quiet0, finite2 and producer18; registry bookkeeping finite1→3→0 and producer0→2→0 remains separate from app-channel reservation depth. Direct egui repaint callbacks and per-frame subsets are separately retained. CPU completion is not display presentation or a measurement of OS repaint latency.

## Original matched G0 controls

Original timer/emitter/oracle bodies and fixture meanings remain unchanged. Full G0 source is `071243db068835617a054a74fb5734a6dd9f128d`. All complete normalized Action/projection, revision, work and FIFO delivery receipts match; root-dependent complete identities are only stable within their owned run.

| Original CPU owner | G0 p50 / p95 / max ms | Current p50 / p95 / max ms |
|---|---:|---:|
| Finite8 | 341.0464 / 419.0257 / 422.8615 | 318.6361 / 373.2622 / 408.3160 |
| Concurrent finite14 | 544.4073 / 599.4350 / 611.4708 | 535.7249 / 574.6788 / 655.9699 |

The concurrent maximum is adverse (+7.28%). No runtime event optimization was made, so these measurements are characterization, not a causal speedup claim. Pooled original ages are finite160 events141.5167/235.5960/322.3859ms and concurrent280 events258.2848/442.6884/549.7643ms. The new ten/eighteen-event real-frame recipes have different boundaries and must not be divided by the original eight/fourteen-event G0 controls.

## Identity, safety and limits

Recipe FNV bindings and full Action/projection identities were reconstructed independently before owner measurement. Full quiet completion was naturally pending17/rebuilt3, while finite/producer were rebuilt20 each. Complete conditional suggestion keys/rows were verified posttimer; no debounce was forced. Full Action identity is stable; projection/state/viewport may legitimately differ across completion states. C2-C retains geometry generation for equal complete results, while search acceptance clears selection independently.

All retained app lifetimes require exactly four watcher registration attempts/creations/final callback releases (clipboard_modify, gui_actions, gui_notes, json_dashboard), with zero send failures, overflow or abandonment and final queue depth zero. App/coordinator teardown and callback release precede owned workspace/environment restoration. Final callback closure release does not prove a join of notify6.1.1 detached Windows backend threads.

Urgent receipts prove actual stale emergency rejection and missing-lease radial rejection before native restore/binding resolution. They do not prove successful native recovery or external action execution. Dashboard backend refresh is suppressed before every actual root frame. No recycle producer, cleanup/query, clipboard, desktop or native input operation ran. All33 variants and sender/order qualifications are in delivery_matrix.md; there is no coalescing whitelist. Existing active unbounded mpsc and preowner256 behavior remain unchanged.

Raw logs stay ignored under target. Sanitized full arrays are in full_frames.json and original_full_controls.json; Small arrays in small_frames.json. Native display/input, hotkey focus/tap-hold, CPU allocator/RSS and workload prevalence are NOT MEASURED. No application-wide performance percentage is claimed.
