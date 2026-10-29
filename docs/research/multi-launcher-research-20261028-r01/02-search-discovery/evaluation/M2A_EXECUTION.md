# M2-A execution receipt — bounded synthetic reference

2026-10-28; M1-C accepted/pushed `ea453edf097b9251414f0a2d24319b454856e81b` before implementation. Application source remains `59156d96adcc73939c7171487a0cb582e3286587`. This checkpoint implements and executes the preregistered reference. Candidate interpretation, evaluated failure examples and final recommendation remain M2-B; this is not a shipping decision.

The [prototype](../prototype/README.md) is Python standard library only. Pure [reference functions](../prototype/reference.py) own synthetic admission, eligibility, integer scoring, fixed Action+args dedup and deterministic output. [Runner](../prototype/evaluate.py), [smoke](../prototype/smoke.py) and [bounded I/O](../prototype/runtime_io.py) read saved research inputs and write research results. They invoke no Rust, provider, native function, network, subprocess or user store. No production source was written. The separate [label remeasurement script](../prototype/remeasure_label_correction.py) never calls the query/ranking functions.

## Execution and budget

Commands were issued from repository root with `python -X utf8 -B`; actual argv and full environment/hash mappings are in each linked receipt. These are the five invocations, in order:

```text
python -X utf8 -B docs/research/multi-launcher-research-20261028-r01/02-search-discovery/prototype/smoke.py
python -X utf8 -B docs/research/multi-launcher-research-20261028-r01/02-search-discovery/prototype/evaluate.py --candidate P0
python -X utf8 -B docs/research/multi-launcher-research-20261028-r01/02-search-discovery/prototype/evaluate.py --candidate P1
python -X utf8 -B docs/research/multi-launcher-research-20261028-r01/02-search-discovery/prototype/evaluate.py --candidate P2
python -X utf8 -B docs/research/multi-launcher-research-20261028-r01/02-search-discovery/prototype/remeasure_label_correction.py
```

| Invocation | Disposition | Guarded seconds / limit | Observed tool process wall seconds | Receipt |
|---|---|---:|---:|---|
| `smoke.py` | COMPLETED, 26 assertions | 0.033678100 / 15 | 0.2902561 | [smoke](results/smoke-initial/receipt.json) |
| `evaluate.py --candidate P0` | COMPLETED | 0.458760500 / 120 | 0.7059054 | [P0 initial](results/P0-initial/receipt.json) |
| `evaluate.py --candidate P1` | COMPLETED | 0.496367300 / 120 | 0.7820422 | [P1 initial](results/P1-initial/receipt.json) |
| `evaluate.py --candidate P2` | COMPLETED | 0.916358400 / 120 | 1.1750476 | [P2 initial](results/P2-initial/receipt.json) |
| `remeasure_label_correction.py` | COMPLETED, sole corrective cycle; zero ranking calls | 0.964564300 / 120 | 1.4862536 | [correction](results/P0-P1-P2-corrective/receipt.json) |

The original reproduction instructions are in [prototype README](../prototype/README.md). That file remains byte-preserved from the initial runs; this receipt adds the separate corrective command without rewriting original code evidence.

Interpreter: Python `3.11.3 (tags/v3.11.3:f3909b8, Apr 4 2023, 23:49:59) [MSC v.1934 64 bit (AMD64)]`; `sys.platform=win32`. No subprocess-based platform helper is used. [Execution ledger](EXECUTION_BUDGET.json) records the attempt sequence, limits and sole consumed correction cycle. Guarded total is **2.869728600140661 seconds / 600**; observed tool process wall times total approximately **4.4395049 seconds**. The guard uses monotonic `perf_counter` from main entry through verification/raw/aggregate writes, excluding interpreter startup and final receipt/ledger writes. Tool wall observations supply the larger process interval, not a second benchmark. No timeout or failed invocation occurred. No corrective smoke was needed because original logic and predictions stayed unchanged. No further correction/evaluation cycle remains.

Each initial candidate invocation processes both scopes and all 300 cases, with identical replay, reversed enumeration and synthetic zero/heavy usage comparisons inside that invocation. It saves 600 base predictions and verifies four further projections per case; there are 1,800 saved initial predictions across the three candidates. Those repeated invariant projections are neither additional candidates nor a timing experiment. Bound checks are cooperative at iteration/operation boundaries rather than an external process-kill service; all observed invocations finished well within their limits.

## Immutable evidence and correction

All 17 `g02-eval-v1` input identities verified before every initial invocation and again before correction. The [original manifest](freeze_manifest.json), original cases/protocol/numeric/runtime/UX/source evidence and all initial outputs remain unchanged. Initial receipts record UTF-8/LF canonical source/input hashes; they do not claim raw-byte equality after Windows/Git normalization. [Exact original source bytes](results/P0-P1-P2-corrective/initial-source-snapshot.json) are recoverable from base64, with raw hashes and canonical hashes. These bytes were captured from unchanged author-owned files at correction time; the initial receipts independently verify canonical identity, not a retroactive raw-byte receipt. The original five source/README files remain in `prototype/` unchanged.

Initial v1 output had one common label disagreement: Q122 `data viewer` was labeled natural/chooser despite the frozen reserved `data` head. It produced no rows under all three policies and both scopes. [Deviation 001](deviations/001-q122-data-head/DEVIATION.md) documents source, before/after labels, counts, parent corroboration and separate [amendment manifest](deviations/001-q122-data-head/amendment_manifest.json). Only Q122 labels/rationale/provenance changed; no query, context, split, catalog or formula changed. The one corrective invocation recomputed measurements over the saved predictions, preserving **1,800/1,800** complete result objects (gate, state, scores, decomposition, order, eligibility diagnostics, operations and full payloads). No relevance-driven tuning or reranking occurred.

| Version | Cases / conditional positives | Gate agreement, every policy/scope | Expected gated | Forbidden-case / desired-abstention denominator | Chooser denominator |
|---|---|---|---|---|---|
| Original v1, retained | 300 / 120; split positives 68/34/18 | 299/300; Q122 mismatch retained | 163 | 180 / 180 | 31 |
| Q122 label v2, amended measurements | 300 / 119; split positives 68/33/18 | 300/300 | 164 | 181 / 181 | 30 |

All original relevance numerators stay unchanged because Q122 had no rows. The altered denominator is always disclosed. Full15 and initial11 retain identical 300 cases, graded acceptable IDs and denominators within each version. V2 initial-subset absent-any is **38/119**, absent-all **23/119**, versus full **0/119** for both. Missing targets remain failures, never removed cases. These are catalog ceilings, not live availability measurements.

## Recorded observations; interpretation deferred

Every value below uses amended v2 labels and is synthetic, author-visible and dependent on the curated corpus. P0 is a lexical surrogate, not current Rust search. The four extra descriptors are research-only; full15 evidence does not expand the eleven-descriptor foundation. Raw per-case files and per-split/family aggregates are under each linked summary's directory.

| Policy / scope | Conditional top1 / top5 (denominator 119) | MRR / nDCG@5 | Holdout top1 (denominator 18) | Positive-typo top1 (denominator 29) | Chooser compliance (denominator 30) | Summary |
|---|---|---|---|---|---|---|
| P0 full15 | 8 / 8 | .06723 / .05399 | 1 | 0 | 0 | [P0](results/P0-P1-P2-corrective/P0/summary.json) |
| P0 initial11 | 7 / 7 | .05882 / .04720 | 1 | 0 | 0 | [P0](results/P0-P1-P2-corrective/P0/summary.json) |
| P1 full15 | 50 / 51 | .42297 / .40746 | 5 | 0 | 3 | [P1](results/P0-P1-P2-corrective/P1/summary.json) |
| P1 initial11 | 39 / 39 | .32773 / .30819 | 5 | 0 | 1 | [P1](results/P0-P1-P2-corrective/P1/summary.json) |
| P2 full15 | 71 / 73 | .60364 / .60109 | 6 | 14 | 7 | [P2](results/P0-P1-P2-corrective/P2/summary.json) |
| P2 initial11 | 58 / 58 | .48739 / .47172 | 6 | 12 | 3 | [P2](results/P0-P1-P2-corrective/P2/summary.json) |

For every policy and scope, v2 synthetic gated preservation is **164/164**, forbidden-suggestion cases **0/181** (also 0/300 overall), ineligible-owner suggestions **0/11**, desired abstention **181/181**, and final-state selected-binding fixtures **10/10**. Payload fidelity, exact-key uniqueness, integer scores, stable score/ID order, output eligibility, gate-no-rows and top5 cap each pass **300/300**, as do identical replay, reversed enumeration and zero versus heavy synthetic usage. These predicates describe the finite fixtures; no GUI transition, current-Rust compatibility or general intent-safety proof follows.

V2 overall synthetic discovery coverage is respectively P0 full/initial **8/300, 7/300**; P1 **51/300, 39/300**; P2 **73/300, 58/300**. The 164 expected gated cases and 17 admitted negatives stay in that denominator. Family/split summaries preserve null/NA for zero denominators. P2 initial runs record 10,446 potential typo pairs/10,391 edit tests full and 8,065/8,020 initial over saved base predictions; no case reaches budget abstention. P0/P1 have zero edit work. These operation observations do not validate a larger catalog, Rust target or application performance.

The low chooser counts, remaining misses, tiny author-visible holdout, finite English vocabulary and absent initial targets require M2-B analysis. There is no human ground truth, independent blind labeler, actual default runtime coverage or native measurement. Hypothetical Complete fixtures do not solve the 43 unclassified global grammars; discovery UI remains NOT_IMPLEMENTATION_READY. A useful prototype alone does not justify unused production metadata/ranker infrastructure. No final candidate, production package, new test campaign or future goal is accepted/executed here.

## Reproduction and checkpoint verification

For an independent fresh copy, use the original commands once in an empty results/ledger location, preserving any existing evidence rather than deleting/overwriting it. The scripts deliberately reject repeated initial/corrective attempts in this saved checkpoint. Existing output is the reproducible record; executing again in this research session would exceed the declared cycle policy even if fast. Recover source bytes from the saved base64 receipt if exact working-copy byte reconstruction is needed; ordinary checkout canonical hashes are sufficient for portable content identity.

[Scoped static reconciliation receipt](M2A_CHECKS.json) records the final freeze/amendment checks, source snapshot reconstruction, raw/aggregate consistency and unchanged-prediction identities. Its checks read saved data and never call a ranker. Parent owns checkpoint commit/push and the next M2-B dispatch.
