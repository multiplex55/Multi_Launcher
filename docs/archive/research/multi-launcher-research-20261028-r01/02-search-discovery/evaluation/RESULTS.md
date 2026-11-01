# Search discovery evaluation — interpretation and decision

M2-B, 2026-10-28. M2-A accepted/pushed `67df35cfc8dd1e136601554741b5ea4358bfd052`. **P2 is the preferred conditional research policy under the frozen criteria. Recommend no production search/UI change now.** Its advantage is fragile, chooser behavior remains weak, and the required usable admission/consumer contract is unresolved. The next proposed work is the bounded SEARCH-F002 admission/useful-consumer decision spike, followed only conditionally by existing SEARCH-F001 metadata. No pure-ranker or UI implementation package is accepted from this evidence.

All numbers are synthetic, model-authored, author-visible and local to this catalog/corpus. P0 is a lexical surrogate, not current Rust search. No current-launcher superiority, human preference, native safety or application latency has been measured. This interpretation reads saved outputs only; it executes no prototype and changes no inputs, code, results or receipts.

## Evidence versions and common denominators

Binding definitions: [frozen protocol](EVALUATION_PROTOCOL.md), [policy data](candidate_policies.json), [dataset](DATASET_README.md), [v1 manifest](freeze_manifest.json), [execution/environment/budget receipt](M2A_EXECUTION.md), [M2-A static reconciliation](M2A_CHECKS.json). Amended measurements: [P0 summary](results/P0-P1-P2-corrective/P0/summary.json), [P1 summary](results/P0-P1-P2-corrective/P1/summary.json), [P2 summary](results/P0-P1-P2-corrective/P2/summary.json). Each summary includes overall, split, family and split×family ratios, denominators, failure IDs and operation counts. Saved raw case files in those directories contain full scored-candidate explanations, emitted payloads, filtered-owner reasons and invariant observations. [Evaluated examples](../EVALUATED_EXAMPLES.md) link exact case lines.

The single correction allowance was consumed in M2-A by a label-integrity error: Q122 `data viewer` conflicted with the already-frozen reserved `data` head. [Deviation 001](deviations/001-q122-data-head/DEVIATION.md), its [before/after](deviations/001-q122-data-head/before-after-Q122.json) and [amendment manifest](deviations/001-q122-data-head/amendment_manifest.json) retain the source justification and separate identities. Original v1 files and all initial outputs remain preserved. Corrective measurement used **zero ranking calls** and preserved **1,800/1,800** complete predictions, including scores/order/payloads. No correction cycle remains and no tuning occurred here.

| Quantity | Retained original v1 | Amended label v2 used below |
|---|---|---|
| Cases; development / validation / holdout | 300; 173 / 61 / 66 | unchanged |
| Conditional positives; split denominators | 120; 68 / 34 / 18 | 119; 68 / 33 / 18 |
| ADMIT / gated | 137 / 163 | 136 / 164 |
| ProtectedSyntax | 103 | 104 |
| Forbidden-case / desired-abstention denominator | 180 / 180 | 181 / 181 |
| Chooser denominator | 31 | 30 |
| Gate agreement, each policy/scope | 299/300; Q122 disagreement | 300/300 |

All relevance numerators remain unchanged: Q122 had no emitted rows. Versioning changes denominators, not ranking quality evidence. Full **15** and initial **11** use the same 300 cases, graded labels, ideal rankings and 119-positive denominator within v2. Four extra Diff/File Search descriptors remain research-only; no added-owner gain is attributed to the eleven-descriptor proposal.

## Conditional relevance and coverage

Top1/top5 denominator is 119; MRR/nDCG@5 average over the same positives, including abstentions and missing targets. Coverage denominator is all 300, including canonical/pending/unsupported/Unknown exclusions and admitted negatives. Chooser is stricter than top1: chooser state plus at least two acceptable distinct Action+args keys. These are separate metrics.

| Policy / scope | Acceptable top1 | Acceptable top5 | MRR | nDCG@5 | Corpus discovery coverage | Chooser |
|---|---:|---:|---:|---:|---:|---:|
| P0 full 15 | 8/119 | 8/119 | .06723 | .05399 | 8/300 | 0/30 |
| P1 full 15 | 50/119 | 51/119 | .42297 | .40746 | 51/300 | 3/30 |
| P2 full 15 | 71/119 | 73/119 | .60364 | .60109 | 73/300 | 7/30 |
| P0 initial 11 | 7/119 | 7/119 | .05882 | .04720 | 7/300 | 0/30 |
| P1 initial 11 | 39/119 | 39/119 | .32773 | .30819 | 39/300 | 1/30 |
| P2 initial 11 | 58/119 | 58/119 | .48739 | .47172 | 58/300 | 3/30 |

Fractional MRR/nDCG numerators and unrounded values are retained in the summaries; displayed decimals are rounded. These policies intentionally abstain below evidence thresholds, so latent candidates below threshold do not count as top5 results. P2 full still misses acceptable top1 on 48/119 positives; initial misses 61/119. Full chooser fails 23/30, initial fails 27/30. Positive classification or deterministic row output does not establish a satisfactory interaction.

| Scope / policy | Development top1 / top5 (68 positives) | Validation top1 / top5 (33) | Holdout top1 / top5 (18) | Split MRR: development / validation / holdout |
|---|---|---|---|---|
| full 15 P0 | 7 / 7 | 0 / 0 | 1 / 1 | .10294 / .00000 / .05556 |
| full 15 P1 | 35 / 36 | 10 / 10 | 5 / 5 | .51961 / .30303 / .27778 |
| full 15 P2 | 47 / 49 | 18 / 18 | 6 / 6 | .70343 / .54545 / .33333 |
| initial 11 P0 | 6 / 6 | 0 / 0 | 1 / 1 | .08824 / .00000 / .05556 |
| initial 11 P1 | 27 / 27 | 7 / 7 | 5 / 5 | .39706 / .21212 / .27778 |
| initial 11 P2 | 38 / 38 | 14 / 14 | 6 / 6 | .55882 / .42424 / .33333 |

Holdout chooser compliance is full P0/P1/P2 **0/12, 1/12, 3/12**, but initial is **0/12 for every policy**. The same holdout top1 count hides this reduced ambiguity coverage and lower initial nDCG (P2 .26886 versus full .33333). Removing targets changes the available alternatives, even when the first acceptable hit count stays equal.

### Family failures and catalog ceilings

All columns below use identical family denominators; entries are acceptable top1/top5 numerator pairs. Families with no conditional positives have **NA**, never 0% relevance or 100% semantic success.

| Family | All cases / conditional positives | P0 full; initial | P1 full; initial | P2 full; initial | Interpretation |
|---|---:|---|---|---|---|
| Natural | 55 / 52 | 4/4; 4/4 | 37/37; 29/29 | 44/44; 35/35 | Alias/role vocabulary helps authored phrasing; Q038/Q048/Q027 still fail in development/validation/holdout. |
| Typo | 32 / 29 | 0/0; 0/0 | 0/0; 0/0 | 14/14; 12/12 | Finite one-edit evidence helps some forms; short tokens, insufficient confidence and synonym gaps remain. Three other typo cases are protected. |
| Ambiguity | 40 / 36 | 4/4; 3/3 | 11/12; 8/8 | 11/13; 9/9 | P2 full top1 equals P1 despite more acceptable later rows. Stable ties can put the wrong tool first; Q132 is a regression. |
| Availability/context | 40 / 1 | 0/0; 0/0 | 1/1; 1/1 | 1/1; 1/1 | Only one positive grid fixture; the rest check gates/negative contexts. Not a general availability-quality sample. |
| Edge | 20 / 1 | 0/0; 0/0 | 1/1; 1/1 | 1/1; 1/1 | The one positive is repetition invariance, Q287. Bounds/Unicode/punctuation examples mostly abstain by construction. |
| Protected | 83 / 0 | NA; NA | NA; NA | NA; NA | Finite guard fixtures, not execution of actual Rust route parsers. |
| Unsafe/unsupported | 30 / 0 | NA; NA | NA; NA | NA; NA | Finite exclusions; no general unsafe-intent understanding claim. |

Initial subset absence is policy-independent and retained in each summary:

| Positive grouping | Absent any acceptable target | Absent all acceptable targets |
|---|---:|---:|
| All | 38/119 | 23/119 |
| Development | 19/68 | 16/68 |
| Validation | 7/33 | 7/33 |
| Holdout | 12/18 | 0/18 |
| Natural | 10/52 | 10/52 |
| Typo | 7/29 | 5/29 |
| Ambiguity | 21/36 | 8/36 |

Availability/edge positives have zero absent targets; protected/unsafe have no positive denominator. Full catalog absence is zero in every positive group. Missing all acceptable targets makes initial success impossible on 23/119 cases; missing some constrains chooser/ideal nDCG. This ceiling is disclosed rather than “corrected” by deleting cases. It does not justify adding the extra four openers without source-owner/test/scope amendment.

## Apply the preregistered criteria

[Protocol §6](EVALUATION_PROTOCOL.md#6-predeclared-decisionno-change-criteria) first rejects unresolved hard-invariant or forbidden-output defects, then compares holdout conditional quality. It does not require high absolute accuracy or a minimum chooser floor; passing it is a conditional selection, not a production acceptance threshold.

| Criterion | Full15 observation | Initial11 observation | Disposition |
|---|---|---|---|
| No unresolved guard/payload/eligibility/determinism/usage defect on declared fixtures | All pass after source-backed label correction; forbidden 0/181 | same | Pass on finite fixtures only |
| P1 ≥5 percentage points holdout top1 gain over P0; no MRR decrease | 5/18 versus 1/18: +22.22 points; MRR .27778≥.05556 | same | P1 qualifies |
| Desired abstention and chooser must not worsen | Abstention 181/181 each; chooser P1 3/30≥0/30 | Abstention equal; chooser 1/30≥0/30 | P1 qualifies |
| Prefer P1 unless P2 adds ≥3 holdout top1 points | P2 6/18 versus 5/18: +5.56 points | same | P2 clears this floor |
| P2 typo top1 gain ≥10 points over P1 | 14/29 versus 0/29: +48.28 points | 12/29 versus 0/29: +41.38 points | P2 clears this floor |
| P2 MRR/abstention/chooser must not regress | Holdout MRR .33333≥.27778; abstention equal; chooser 7/30≥3/30 | Holdout MRR equal to full; abstention equal; chooser 3/30≥1/30 | P2 qualifies |

**Conditional selection: P2 in both catalog scopes, with no tuning.** P0 remains the preserved surrogate baseline; P1 is the simpler qualifying fallback. This does not imply a future shipping implementation should automatically port P2. The current production recommendation remains **no search/ranker/UI change until a concrete admissible consumer/compatibility decision exists**.

### One-case sensitivity, wins and regressions

All six P2 holdout hits are Q025/Q026/Q028 (XML group) and Q111/Q112/Q113 (comparison group). P1 gets the same set except **Q026, `format xml`**; P0 gets Q112. Thus the P2–P1 holdout preference depends entirely on one authored vocabulary case, with a 5.56-point step per hit. If that sole gain is not confirmed by independent intent judgments, the observed holdout gain disappears and the predeclared simplicity preference selects P1. This is a sensitivity interpretation, not a relabeled/rerun evaluation. There is no significance claim and no holdout tuning.

Saved top1 comparison over all v2 positives: P0→P1 full has **42 gains/0 losses**, initial **32/0**; P1→P2 full has **22 gains/1 loss**, initial **19/0**. The full loss is Q132 `search files`: P1 puts acceptable File Search content first at 80; P2 gives Duplicate Finder and three File Search openers90, and ASCII-ID order puts Duplicate Finder first. Q131 already suffers that tie in P1/P2; Q138 `file finder` confidently suggests Duplicate Finder even though labels accept File Search. Zero forbidden output does not mean zero semantically wrong suggestion: these positive cases do not mark every irrelevant ID forbidden. [Detailed examples](../EVALUATED_EXAMPLES.md) preserve the decomposition and distinction.

Chooser compliance is too weak to treat broad wording as resolved: 7/30 full P2 and 3/30 initial, with no initial holdout chooser success. Candidate quality thresholds were deliberately relative; retroactively inventing a new passing floor or lowering confidence thresholds would be tuning. Preserve these outcomes and do not claim implementation-ready UI.

Some positive labels are contestable independently of implementation. Q148 `test` accepts Regex Tester, and P1/P2 suggest it at 80/75; that broad word could mean unrelated testing to a real user. Q138 `file finder` accepts File Search while the catalog vocabulary favors Duplicate Finder; an independent labeler might treat that phrase differently. Q122 was a concrete contradiction of predeclared source-backed syntax; these semantic judgments are not comparable correction defects. Keep their original labels and count them honestly rather than using the exhausted correction allowance to improve agreement.

## Gates, eligibility, payload and state evidence

For each policy/scope, amended synthetic gates agree 300/300. Expected non-ADMIT cases are 164: ProtectedSyntax104, InputBound11, Disabled2, ExcludedSurfaceOrOrigin12, BusyOrStale11, CoverageUnknown3, NoEligibleOwner1, UnsupportedIntent20. There are 136 ADMIT cases:119 positives and 17 negatives. All remain in 300-case coverage. Protected syntax is checked before natural content bounds under the frozen ordering; raw byte bound still comes first.

Every policy/scope records:

- Synthetic gated preservation 164/164; forbidden suggestion case rate0/181 and 0/300 overall; desired abstention 181/181.
- Protected-family forbidden 0/83, unsafe-family 0/30, availability-family 0/39; other family denominators remain in summaries.
- Ineligible-owner suggestions 0/11; owner restrictions include missing/disabled/denied capability/origin/epoch/identity cases, not just an enabled boolean.
- Exact source payload, action-key uniqueness, integer scores, stable score/ID ordering, output eligibility, gate-no-rows and top5 cap 300/300 each.
- Identical replay, reversed descriptor enumeration and zero versus 1,000,000 synthetic usage 300/300 each. Zero usage is policy, not evidence of useful personalized ranking.
- Selected-binding final-state fixtures 10/10; these check already-stale/disabled/origin/surface states, not a real GUI selection lifecycle or native activation.

The raw `forbidden_operations` labels are nonexecution requirements, not proof that an OS command path was tested: the reference has no effect dispatcher. Full Action+args payload fidelity is meaningful for fixed identity preservation; authority, destination state and actual side effects remain governed by source owners and future integration checks. Opening an existing tool may retain state; the prototype does not test or promise a blank/no-work opener.

Gate success is not actual Rust route preservation, safety against arbitrary unsupported wording, live provider fidelity or GUI compatibility. Punctuation/multilingual/short input exclusions create honest false negatives. Matching a few known heads is not the complete protected-prefix inventory. Unknown includes disabled owners, and manager-global content matching must be distinguished from special grammar by source, not by an empty prefix list.

## Limits, resources and next work

The same author designed vocabulary/policies, authored all labels and read every split. Whole-group separation limits direct near-template leakage, but cannot make the holdout blind or independent of chosen source vocabulary. There are only 18 conditional holdout positives, concentrated in XML/comparison meanings; all 40 availability cases are development, and there are no validation unsafe/edge/availability cases. The saved split×family summaries expose these zero cells. Do not generalize state/safety success across unseen template groups. There is no human study or real user query distribution.

Original code/input identities and all outputs were checked in M2-A. Guarded execution 2.8697s/600 is a receipt of this finite Python workload, not an app speedup or Rust≤2ms proof. P2 full base predictions total 10,446 potential typo pairs/10,391 edit tests; initial 8,065/8,020, with zero budget abstentions. No larger catalog or new worker/cache is justified by those counts. Runtime/resource/native questions stay future evidence; missing proof does not block completing research.

The [routing declaration survey](../roles/orchestrator/ROUTING_METADATA_SURVEY.md) has81 registered types:36 prefix-gated and 45 manager-global, of which43 global body contracts remain unclassified beyond Web/Calculator. Source defaults are None/None plugin/capability policy; no actual user configuration was observed. Complete fixtures are hypothetical; they establish neither useful admission under defaults nor a safe route manifest. A favorable source-only classification alone cannot establish live/native coverage or authorize production infrastructure.

### Finite disposition for M3-A packaging

| Work | Decision and boundary | Packaging obligation / dependency |
|---|---|---|
| **SEARCH-F002 admission/useful-consumer decision proof** | Proposed next action. Bounded source/contract proof using actual81-owner inventory, including43 unresolved global grammars, disabled owners/dynamic Unknown, revision/invalidation, real origin surfaces and canonical pending/deferral rules. Must choose a concrete useful consumer/compatibility contract or explicit no-change; no broad plugin framework, production route rewrite or default implementation. | Author full raw/canonical Goal/Kickstart at M3-A; PROPOSED_NOT_AUTHORIZED. Positive source analysis is necessary but insufficient: acceptance must explicitly name the concrete consumer and compatibility evidence still required before infrastructure execution is recommended. |
| **SEARCH-F001 metadata foundation** | Existing planner package retained conditionally, initial 11 only. Do not build unused descriptors merely because conditional relevance increased. | Preserve raw planner pair; amend both canonical documents so accepted concrete consumer/compatibility outcome of SEARCH-F002 precedes execution recommendation. Single canonical module owner `src/search_discovery`; reconcile raw/research bounds in a separate amendment, never frozen input edits. |
| P2 pure production ranker | Preferred conditional research reference; implementation recommendation unaccepted. | No orphan ranker package or automatic port. A later accepted consumer could support a new explicitly scoped authorization; current research is complete without it. |
| Opt-in discovery UI | NOT_IMPLEMENTATION_READY; production action unaccepted. | No UI package claims readiness; complete route coverage, origin, immutable binding/eligibility, canonical/Macro exclusions and owned settings remain concrete acceptance boundaries for the decision proof. |
| Four research-only Diff/File Search descriptors | Coverage only, no shipping expansion | Keep full/initial metrics separate; canonical F001 remains11 unless a future explicit source/test/scope decision changes it. |
| Personalization, additional corpus/tuning, dispatcher/plugin ABI, new cache/worker | No action accepted | No separate package; no additional experiment this session. |
| AUD-F001/AUD-F002 | Reuse existing corrected audit packages | No duplicate terminal/file-authority goals; neither is a discovery prerequisite without new concrete dependency. |

M3-A is finite packaging and reconciliation of this decision, not another research campaign. All future packages remain PROPOSED_NOT_AUTHORIZED and start from then-current master only when separately invoked. No future goal is executed or master merge authorized by this report.

## Per-case emitted-top1 gains and losses

These lists compare the same v2 labels and saved emitted results; abstention and absent targets remain failures. A gain/loss means acceptable first-row success changes, not that every score improves. Q038 is an example of a lower useful score that this binary comparison cannot expose. Raw case lines and exact failure IDs remain authoritative.

| Scope / comparison | Gain IDs | Loss IDs |
|---|---|---|
| full P0→P1 | Q004, Q005, Q006, Q007, Q008, Q014, Q015, Q016, Q025, Q028, Q034, Q035, Q036, Q045, Q046, Q055, Q056, Q064, Q065, Q074, Q075, Q076, Q084, Q085, Q086, Q094, Q095, Q096, Q104, Q105, Q106, Q107, Q108, Q111, Q113, Q133, Q140, Q148, Q149, Q150, Q210, Q287 | None |
| full P1→P2 | Q009, Q010, Q017, Q019, Q026, Q039, Q040, Q047, Q050, Q057, Q059, Q067, Q070, Q077, Q079, Q089, Q090, Q097, Q110, Q126, Q273, Q279 | Q132 |
| initial P0→P1 | Q004, Q005, Q006, Q007, Q008, Q014, Q015, Q016, Q025, Q028, Q034, Q035, Q036, Q045, Q046, Q055, Q056, Q064, Q065, Q074, Q075, Q076, Q084, Q085, Q086, Q111, Q113, Q148, Q149, Q150, Q210, Q287 | None |
| initial P1→P2 | Q009, Q010, Q017, Q019, Q026, Q039, Q040, Q047, Q050, Q057, Q059, Q067, Q070, Q077, Q079, Q089, Q090, Q126, Q273 | None |
