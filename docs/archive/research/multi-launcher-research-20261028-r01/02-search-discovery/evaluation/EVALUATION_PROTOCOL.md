# Frozen evaluation protocol — g02-eval-v1

M1-C preregistration, 2026-10-28. Source `59156d96adcc73939c7171487a0cb582e3286587`; accepted M1-B `cc3da667348df76abdd81bb4057a0df8393089fc`. No ranker/evaluator code or execution at this checkpoint. Input identities are in [freeze manifest](freeze_manifest.json); [dataset schema/counts](DATASET_README.md), [splits](splits.json), [policies](candidate_policies.json), [ranking](../RANKING_SPECIFICATION.md) and [runtime limitations](../RUNTIME_AND_INTEGRATION_DESIGN.md) are binding inputs.

## 1. Question, candidates and evidence levels

Question: under explicitly synthetic/current eligibility and hypothetical Complete admission, do curated aliases or finite intent/object/one-edit evidence improve **fixed tool-opener suggestions** over a label/shortcut lexical surrogate without violating the declared guards? Exactly P0/P1/P2, formulas unchanged from M1-B. Neither P0 nor this harness reproduces Rust fuzzy/provider ranking, real canonical outcomes, actual UI, native focus, worker retirement or human intent.

Separate evidence levels: plausible source/design; prototype correct on declared invariants; relevance supported on these author-visible synthetic labels; production/live behavior. Only actual evidence may be claimed. A successful conditional prototype does not close the43-global-grammar/default-like reachability gap or justify shipping unused metadata/ranker infrastructure. UI remains NOT_IMPLEMENTATION_READY absent a separate concrete admission decision/proof.

## 2. Frozen input and comparison rules

Read frozen catalog, cases, split membership, policy/role/alias dictionaries and selected route fixtures. All three candidates share the same gates/eligibility/exact Action identities and cases. Evaluate **full 15** and **initial 11** scope within the same candidate run. Do not remove cases or shrink labels/ideal rankings to the initial subset: a missing acceptable target is absent, not silently replaced, and counts against common quality/coverage denominators. Report absent-any/all-acceptable-target counts by split and family for each subset before interpreting metrics.

Field normalization/matching/scoring/dedup/top-k/result states follow RANKING_SPECIFICATION exactly, including final-content-token-only prefix, one corrected query token/descriptor, budget-wide abstention, integer floors and zero usage. No descriptions/action wire strings as retrieval fields. Existing alias/role terms are hypotheses chosen before frozen labels; no extra candidate, synonym expansion, threshold search or post-holdout tuning.

Common gate diagnostic order is frozen for fixture agreement:

1. Feature false→Disabled.
2. Input closed, non-current session, origin other than FreshUserEdit, or surface outside LauncherList/LauncherGrid→ExcludedSurfaceOrOrigin.
3. Settings transition, non-current query generation/stamps, canonical_result_count>0, pending, deferral other than None, or unstable result catalog→BusyOrStale.
4. Route coverage not Complete or route revision non-current→CoverageUnknown.
5. Raw UTF-8 length>256→InputBound; known case-insensitive whitespace head or raw marker/adjacent prefix, disallowed character/hyphen placement→ProtectedSyntax. Check protected shape before natural token limits, so a long URL token remains protected. Empty input and natural raw/token/content bounds→InputBound.
6. Finite dangerous/unsupported rules after defined normalization→UnsupportedIntent.
7. Build eligible descriptor projection (present/unique/Builtin/current epoch/enabled/declared and configured search); if no eligible record remains→NoEligibleOwner. Otherwise ADMIT. An unavailable intended owner alone does not reject the query when unrelated eligible records remain; those negative fixtures require abstention/no forbidden owner suggestions.

This order resolves diagnostic precedence, not canonical behavior. Selected route fixtures are finite source-inspired guards and may be used only as synthetic admission tests. There is no execution of source parsers or claim complete live syntax coverage. The current runtime proposal includes disabled owners in coverage; a Hash-only allowlist cannot turn Unknown into Complete.

## 3. Outputs required from later prototype

Preserve raw per-case records for each candidate/subset, including input ID/split/family/group, predicted gate, filtered owner IDs/reasons, result state, ordered visible IDs/full fixed Action+args, integer score decomposition/matched fields/roles and forbidden/invariant flags. No executable effect occurs. Outputs may refer to synthetic query by ID rather than duplicate it; no private data exists in inputs.

Record command/Python version/platform, candidate/policy/source/input canonical hashes, run disposition, elapsed monotonic wall time, operation counts/budget abstentions and failures. Emit per-split/per-family metrics plus aggregate counts; retain zero denominators as null/NA, never0% success. Keep failed outputs under a distinct attempt directory. Python timing is a surrogate observation only; it does not establish2ms Rust target, GUI responsiveness, native opener speed or app RSS/startup gains.

## 4. Exact numerators/denominators

For case c, A(c)={IDs with frozen grade≥2}; R(c) is the emitted, deduplicated visible ordered list, at most5. No emitted row for abstention. A(c), expected gate/behavior and all denominators remain identical across policies/subsets. Production target resolution is not evaluated.

| Metric | Exact definition / denominator |
|---|---|
| Gate agreement | Count predicted gate exactly equals expected gate / all 300, separately by split/family. Gate failure cannot reduce another metric's frozen denominator. |
| Protected/gated fixture preservation | Expected non-ADMIT cases with exact gate and empty R / all 163 expected non-ADMIT cases. Label explicitly “synthetic gate preservation”, not Rust compatibility percentage. |
| Conditional positive top1 | Count first R ID∈A / all 120 frozen ADMIT cases with A nonempty; split denominators 68/34/18. Abstention, incorrect gate and absent targets count 0. |
| Conditional top5 | Count R intersects A / same 120. k is actual emitted cap5, not an internal unqualified list. |
| MRR | Sum reciprocal rank of first acceptable emitted ID (0 if absent) / same 120. |
| nDCG@5 | DCG=sum((2^grade−1)/log2(rank+1)) over R[:5]; ideal sorts **all frozen graded IDs**, including absent subset targets, by grade then ID, takes5. Divide by ideal DCG, then average over same 120. Grades1 can get partial nDCG but never acceptable top1/top5. |
| Overall discovery coverage | Conditional-positive cases with an acceptable emitted result / all 300. This is synthetic corpus coverage, not accuracy or live/default coverage. Report canonical/pending/coverage exclusions beside it. |
| Forbidden suggestion case rate | Cases emitting any frozen forbidden ID / cases with nonempty forbidden_ids, plus numerator/all 300. Report protected, unsafe and availability families separately. |
| Disabled/denied-owner suggestion | Cases emitting an ID whose owner is ineligible under its frozen context / cases with any ineligible-owner override or all-owner disable. Report count and denominator; no replacement owner by name. |
| Desired abstention | Cases with expected behavior ABSTAIN and empty R / all expected ABSTAIN cases, including admitted negatives. A stronger-looking wrong tool does not count. |
| Ambiguity compliance | Expected CHOOSER cases with chooser state and at least 2 emitted distinct action+args keys whose IDs∈A / all expected CHOOSER cases. Missing subset targets remain failures; label this ceiling separately. |
| Permissive positive behavior | Expected SUGGEST_OR_CHOOSER cases with one of those states and top1∈A / those cases. Distinct from all conditional positive accuracy. |
| Absent-target ceiling | Count positive cases with any/all acceptable IDs absent from subset / same 120, broken down by split/family. Never relabel a missing target to make11 appear equivalent to15. |
| Determinism / usage | Exact equality of gate/state/ordered IDs/scores/full payload under identical replay, reversed descriptor enumeration and synthetic counts 0 versus1,000,000; number passing / number replayed. Zero usage in all policies; this is not a fourth candidate. |
| Selection boundary fixture | Cases with prior_explicit_selection must reject stale/disabled/surface/origin state without retargeted output/activation; count passing / those 10 cases, with the admitted unavailable-owner negative still requiring empty R. This tests final-state fixtures only, not a GUI transition implementation. |

Also list per-case wins/regressions and at least one failure from each nonempty family/split; do not report a single average as proof of safe semantic understanding. Gate failures and forbidden suggestions are separate from relevance misses. No p-value/significance claim from these small dependent authored groups.

## 5. Bounded execution and corrective policy

M2-A may implement standard-library-only Python under prototype/, with no subprocess/network/provider/OS/application/clipboard/real-file actions. Reads are frozen research inputs; writes only research outputs. It must verify hashes before running and abort on drift. No Rust/dependency/native work.

- One small smoke/invariant invocation,≤15 seconds; no relevance evaluation hidden there.
- One initial evaluation invocation **per candidate**, at most3 invocations. Each processes both scopes and all splits without tuning;≤120 seconds each.
- At most **one concrete corrective cycle** for a documented harness/spec transcription or label-integrity defect, with old failure evidence preserved. Rerun only the affected candidate(s) within a single≤120-second corrective invocation; do not rerun all three merely to improve scores. One post-fix smoke≤15 seconds is permitted. If the affected work cannot fit, preserve NOT_RUN/incomplete limits rather than exceed budget.
- Total prototype execution≤10 minutes; predeclared maximum here is510 seconds (15+360+120+15), leaving startup/error margin within600. Record actual durations cumulatively; tool/availability failures do not authorize dependency installs or an unbounded retry loop.

Repeated determinism/usage checks occur inside each bounded candidate run. No repeated timing benchmark, warmup grid or hyperparameter search. If execution is unavailable/too slow, preserve complete code/data/protocol, errors and NOT_RUN metrics without fabricated output; continue interpretation/packages with that limitation.

## 6. Predeclared decision/no-change criteria

First reject an implementation for any unresolved hard gate/payload/eligibility/determinism/usage/retarget invariant defect. A genuine defect can use the single correction cycle; semantic relevance disappointment is not a harness defect. Any forbidden suggestion in protected/unsafe/unavailable fixtures prevents recommending that candidate unchanged; report no-change/spike instead of weakening labels. Not all failed semantic expectations mean code is wrong.

For quality, report all splits but use holdout for final conditional comparison. A richer policy must improve holdout acceptable top1 by at least 5 percentage points over P0, not lower MRR, and not worsen desired abstention or chooser compliance. This is a declared product threshold, not statistical significance. If P1 qualifies, prefer it over P2 unless P2 adds at least 3 points holdout top1 over P1 and improves typo top1 by at least 10 points on the 29 frozen conditional-positive typo cases (the other 3 typo cases are protected) without the above regressions. Report exact small numerators, whole-group dependencies and label uncertainty; a threshold crossed by one case is fragile evidence.

These thresholds can select **no ranking change**. If all candidates fail, retain useful failure evidence, narrower design/spike or no production recommendation. Full-scope gain does not recommend initial 11 shipping; report initial-scope evidence and absent-target ceilings independently. Even a selected conditional policy does not justify a production ranker/metadata package without an accepted usable consumer. M2-B/M3-A must place the admission/completeness decision first when UI remains unresolved, and every accepted implementation/spike needs complete role/canonical pairs. No future goal executes automatically.

## 7. Portable freeze and deviations

Canonical hashing `utf8-lf-text-v1`: decode strict UTF-8, reject BOM, replace physical CRLF with LF, reject remaining physical CR, encode UTF-8, preserve every other character/key order/space/final newline. SHA-256 those bytes and record canonical length. Do not parse/reformat JSON, Unicode-normalize, trim or conflate working-copy raw hashes with Git blob bytes. Escaped JSON query `\r`/`\n` values remain literal escape bytes in the file. The manifest excludes itself; no self-hash cycle. Mutable README/role/progress/evaluation outputs are excluded and named as such.

Manifest covers all declared algorithm/catalog/corpus/split/fixture/protocol/UX/safety/source-evidence inputs at freeze, with relative goal-root paths and versions. Verify every entry before evaluation; external page contents are not fetched/rehashed, only the saved evidence record and narrow citations. Any post-freeze input correction creates a new manifest version/deviation record with reason, old/new identities and affected cases/runs; do not silently overwrite a failed run. Label or formula changes after exposure are exploratory unless the single concrete corrective allowance applies. Parent publication precedes all M2-A implementation/evaluation.

Frozen policy/schema files are evaluator authority, with the linked numerical specification as their interpretation. If they disagree, stop before evaluation and record a concrete versioned correction; do not choose whichever interpretation improves a score. Preserve original v1 files/manifest through M2/M3. Later source-module/bound canonical reconciliation belongs in a separate linked amendment or versioned copied input set, never a rewrite of frozen historical evidence. Mutable decisions/results/index/progress are explicitly outside the freeze.
