# Evaluated examples — saved predictions, including failures

M2-B, 2026-10-28. These examples interpret existing [amended v2 measurements](evaluation/RESULTS.md), not new query executions. Inputs/code/raw outputs remain immutable. Candidate comparison is conditional on synthetic eligibility and hypothetical Complete coverage. P0 is a surrogate; no example runs current Rust search or activates a tool. IDs map to exact JSONL line numbers (Q009 is line 9). Scores below come directly from saved decompositions, not newly computed ranking.

The source-backed catalog contains fixed openers. `SUGGEST_OPEN_TOOL_NOT_ACTIVATE` means explicit user selection is still required; it does not mean the requested operation happened. Payload identity is the complete saved Action with its optional args, whose wire/args portions are shown for clarity. No query text becomes data, a file path, clipboard content or shell input.

## Alias coverage: Q004 `inspect json`

Development/natural, acceptable `tool.json.open` grade 3. [P0 line 4](evaluation/results/P0-P1-P2-corrective/P0/cases-full.jsonl#L4), [P1 line 4](evaluation/results/P0-P1-P2-corrective/P1/cases-full.jsonl#L4), [P2 line 4](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L4).

| Policy / target | Saved evidence | Saved arithmetic | Output |
|---|---|---|---|
| P0, JSON open/format candidates | `json` exact100, `inspect` unmatched; H100,m2,C50,U1,E0 | floor(4×50/5)−10=30 | No qualified row |
| P1, JSON open | Both tokens exact through alias `inspect json`; H200,m2,C100,U0,E1 | 80+20=100 | JSON open100 |
| P2, JSON open | Same alias; QI/DI inspect and QO/DO json; I100,O100 | 60+15+15+15=105, clamped100 | JSON open100 |

Saved payload wire is `json_utility:open`, args null. Curated vocabulary helps this authored phrase; it does not establish general semantic parsing. P2 scores JSON format 12 because inspect conflicts with the format intent, preserving the distinction between entering the general tool and selecting a mode.

## Bounded typo gains and an absolute confidence failure

Q009 `pretty pritn json` is development/typo, grade 3 JSON format. [P1 line 9](evaluation/results/P0-P1-P2-corrective/P1/cases-full.jsonl#L9), [P2 line 9](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L9). P0/P1 exact pretty+json contribute 200, unmatched pritn contributes 0: m3,C66,U1,E0, score floor(4×66/5)−10=42, no row. P2 permits one adjacent transposition `pritn`→`print` at 55: H255,m3,C85,U0; role bonuses I100/O100 add 30 to coverage 51, score 81. It emits `tool.json.format`, wire `json_utility:format`, args null. No JSON is supplied or transformed from the query.

Q273 `minfy json` is another development typo with [saved P2 line 273](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L273). Exact json100 plus corrected minfy→minify 55 gives H155,m2,C77; coverage 46+intent15+object15=76. JSON minify 76 beats JSON format 37, so a fixed minify-mode opener is suggested. P0/P1 target score 30 abstains. This illustrates a meaningful gain, not a reason to infer an operation payload.

Q020 `checksumm` and Q271 `cheksum` expose the limitation ([P2 line 20](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L20), [line 271](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L271)). Both correct to checksum at 55; H55,m1,C55,U0,E0, with compute_hash/hash roles gives 33+15+15=63. A lone 63 row is below the clear-suggestion 75 threshold and has no second qualified alternative, so output is `ABSTAIN_LOW_EVIDENCE`, with no row. They remain relevance failures. Lowering the threshold after seeing them would be post-freeze tuning; no such repair is made.

Q272 `checks file` ([P2 line 272](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L272)) has final content token `file`; `checks` cannot receive a prefix match because it is not final. Hash target H100,m2,C50,U1,E0,I0,O100 gives 30−8+15=37. It abstains. This is a deliberately narrow completion rule, not a typo engine silently completing every word.

## The sole P2–P1 holdout gain: Q026 `format xml`

Holdout/natural, grade 3 XML open. [P1 line 26](evaluation/results/P0-P1-P2-corrective/P1/cases-full.jsonl#L26), [P2 line 26](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L26).

P1 XML only matches xml: H100,m2,C50,U1,E0 →40−10=30. `format xml` is not a frozen XML alias, so P1 abstains. P2's XML intent vocabulary provides exact format plus exact xml: H200,m2,C100,U0,E0,I100,O100 →60+15+15=90. JSON format is7 (30−8+15−30 object conflict), so XML open90 is clear. Payload is `xml:open`, args null; this opens the existing toolkit and does not apply formatting to unspecified XML.

The two scopes both retain XML, so both show this gain. Holdout P0/P1/P2 top1 is1/18,5/18,6/18. P2's sole additional hit over P1 is this one source-vocabulary case, grouped with other XML phrasing. It clears the frozen3-point preference floor in 5.56-point increments, but does not support a confident general-quality claim. A different independent intent judgment on this gain can remove the preference without any code defect.

## Same score, different admissible presentation: Q027 and Q126

Q027 `browse an xml tree` is holdout/natural ([P2 line 27](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L27)). Normalized tokens browse/tree/xml give two exact hits and one unmatched: H200,m3,C66,U1,E0,I100,O100. Score39−8+15+15=61. XML is the lone qualified candidate; the reference abstains rather than show a weak single suggestion. P1 XML score 6 also abstains. This is a natural wording false negative, not protected syntax.

Q126 `view comma data` is validation/ambiguity, with CSV and Table each grade 2 ([P2 line 126](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L126)). Both recognize view and comma but not data, with the same H200,m3,C66,U1,E0 and inspect/csv roles. Both score 61. Because two distinct acceptable payload keys qualify, state is `CHOOSER_EXPLICIT_SELECTION`, CSV then Table by stable ID; wires `csv:open` / `table:open`, args null. P1's CSV score 6 and Table lower evidence abstain. This passes chooser compliance in both scopes. It does not imply that other vague data requests work.

Q048 `show tabular data` is validation/natural ([P2 line 48](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L48)): show is a stopword, tabular/data have no recognized frozen evidence. Table H0,m2,C0,U2 gives unclamped−16, clamped0; all policies abstain. Contrast with Q126 shows how much the outcome depends on exact authored vocabulary.

## A real conditional relevance regression: Q132 `search files`

Development/ambiguity, acceptable File Search open grade 3 and filename/content grade 2. [P0 line 132](evaluation/results/P0-P1-P2-corrective/P0/cases-full.jsonl#L132), [P1 line 132](evaluation/results/P0-P1-P2-corrective/P1/cases-full.jsonl#L132), [P2 line 132](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L132), [initial P2 line 132](evaluation/results/P0-P1-P2-corrective/P2/cases-initial.jsonl#L132).

P0/P1 give the three File Search records 80 from exact file+search: H200,m2,C100,U0,E0. P1 therefore emits an acceptable first row. P2 treats search as find intent and file as object for both File Search and Duplicate Finder. Each has H200,m2,C100,U0,E0,I100,O100, score 60+15+15=90. Full output order:

1. `tool.duplicates.open`90, `dff:open`, args null — not acceptable under this case's labels.
2. `tool.file_search.content`90, `file_search:mode:eyJraW5kIjoiY29udGVudCJ9`, args null.
3. `tool.file_search.filename`90, `file_search:mode:eyJraW5kIjoiZmlsZSJ9`, args null.
4. `tool.file_search.open`90, `file_search:open`, args null.

State is chooser; action identity/order is deterministic, yet top1 becomes wrong under the labels. Top5 still passes. Stable IDs prove determinism, not intent correctness. A score tied across domain meanings is not a confidence signal. Explicit selection prevents an automatic effect; it does not repair poor first-row relevance.

Initial11 contains no File Search descriptor. It can still suggest Duplicate Finder 90 while every acceptable target is absent; the case remains a failure with the same label/denominator. In initial comparison P1 was already unsuccessful, so the extra P2 full regression disappears from the *delta* count without becoming a good suggestion. This is why scope-specific deltas and absent-target accounting must be interpreted together.

Q131 `find files` ([P2 line 131](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L131)) has the same P2 tie and wrong first row; P1 already ties Duplicate Finder first at 80. Q138 `file finder` ([P2 line 138](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L138)) is worse for presentation: Duplicate Finder 75 is the sole clear suggestion (coverage 60+file object15); acceptable File Search candidates 37 are absent from visible rows. The frozen aliases/role vocabulary do not resolve this domain ambiguity. The case has no forbidden-ID set, so zero forbidden suggestions coexists with a confidently wrong tool under the semantic labels. Neither label nor alias is modified after exposure.

## Intent conflict can reduce plausible results: Q038

Development/natural `inspect regular expressions`, acceptable Regex Tester grade 3 ([P1 line 38](evaluation/results/P0-P1-P2-corrective/P1/cases-full.jsonl#L38), [P2 line 38](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L38)). Finite inflection maps expressions→expression. P1 matches regular+expression but not inspect: H200,m3,C66,U1,E0 →52−10=42. P2 still matches those two terms and recognizes QI inspect/QO regex. Regex descriptor DI test_regex does not overlap inspect, while object matches: coverage 39−8+object15−intent-conflict25=21. Both abstain; P2 makes the plausibly relevant candidate weaker. This is a score regression even though the binary emitted-top1 metric cannot fall below failure. Distinguish aggregate top1 gains from preservation of every useful lexical ordering.

Validation typo Q099 `compare directroies` ([P2 line 99](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L99)) matches compare but leaves directroies unmatched: Diff37. The frozen correction rules do not recover it. Do not describe bounded typo support as robust misspelling recovery.

Holdout ambiguity Q114 `find differences` ([P2 line 114](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L114)) expects Image Comparison/Diff chooser. P2's strongest irrelevant find/file owners score 37; intended Diff receives an intent conflict because find does not equal compare. No row qualifies. Related Q115–Q120 remain failure cases. Full chooser7/30 and initial 3/30 are weak absolute results despite improvement over P1.

## Protected, unsupported and unavailable cases are separate evidence

The [saved case file](evaluation/deviations/001-q122-data-head/queries-v2.jsonl) and raw predictions give the following examples. These guards are common to all three policies and both scopes; empty rows are expected, not relevance failures or proof of actual parser preservation.

| Case / split | Synthetic input/state | Saved disposition | What it establishes / does not establish |
|---|---|---|---|
| Q001 development | `json` explicit head | ProtectedSyntax, no rows | Selected head guard; no Rust provider was called. |
| Q041 validation | `table` explicit head | ProtectedSyntax, no rows | Another selected route; not complete inventory. |
| Q021 holdout | `xml` explicit head | ProtectedSyntax, no rows | A held-out fixture shares known route metadata. |
| Q122 validation | `data viewer` reserved head | ProtectedSyntax, no rows | Source-backed label correction; original ADMIT/chooser error retained in v1. |
| Q211 development | `pretty print json`, provider pending | BusyOrStale, no rows | Does not reinterpret waiting as no match; not live provider retirement. |
| Q212 development | Same, capacity deferral | BusyOrStale, no rows | Deferral blocks discovery even with zero canonical rows. |
| Q213 development | Same, one canonical result | BusyOrStale, no rows | Canonical precedence; no actual search vector was constructed. |
| Q202/Q204 development | Programmatic/Automatic origin | ExcludedSurfaceOrOrigin | Does not feed discovery into programmatic/Macro search; origin type remains proposed. |
| Q206/Q207 development | Radial/Dashboard surface | ExcludedSurfaceOrOrigin | Research excludes these surfaces; does not change their existing grammar. |
| Q221/Q222 development | Image owner disabled / capability restriction empty | ADMIT but no visible rows | Other owners remain eligible; no forbidden image-owner substitution. |
| Q230 development | All owners disabled | NoEligibleOwner | No projection to rank; no live plugin reconfiguration. |
| Q218 development | Route coverage Unknown | CoverageUnknown | Hypothetical context fails closed; no proof a real configuration reaches Complete. |
| Q231/Q237 development | Prior selection, stale stamp / File Search disabled | BusyOrStale / ADMIT with no rows | Final-state rejection/abstention; does not implement a GUI transition or retarget prevention. |
| Q251 development | `delete duplicate files` | UnsupportedIntent | Finite dangerous verb rule; no deletion capability advertised. |
| Q241 holdout | `extract zip` | UnsupportedIntent | Unsupported operation stays unadvertised; no ZIP extractor invented. |
| Q281 holdout | Empty query | InputBound | No natural request to rank. |
| Q286 development | Overlong repeated checksum tokens | InputBound | Raw/content admission bound, not a workload benchmark. |

All protected-family83, unsafe-family 30 and availability forbidden 39 fixtures emit no forbidden ID. This is not a universal safety guarantee: positive semantic mistakes such as Q138 are outside those forbidden sets, and unknown unsafe paraphrases are not evaluated. The reference has no effect executor; `forbidden_operations` describes required nonexecution, not an OS action test.

Q210 grid `open checksum tool` and Q287 `checksum checksum` are the only conditional positives in availability/edge respectively. Both yield Hash 100 under P1/P2 with payload `hash:open`, args null ([P2 line 210](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L210), [line 287](evaluation/results/P0-P1-P2-corrective/P2/cases-full.jsonl#L287)). The repeated content token deduplicates to one checksum; P2 unclamped 105 clamps 100. They are useful invariant fixtures, not broad quality samples.

## Coverage of failure evidence by split and family

| Family | Development | Validation | Holdout |
|---|---|---|---|
| Natural | Q038 role-conflict/phrase miss | Q048 vocabulary miss | Q027 low single-row evidence |
| Typo | Q020/Q271 confidence failure; Q272 final-token restriction | Q099 uncorrected word | Q029 `validate xlm` short token outside correction; Q276 similar |
| Ambiguity | Q132 top1 regression; Q138 wrong clear tool | Q121 `inspect structured data` no match; Q126 success is retained as contrast | Q114 missing chooser; Q115–Q120 related failures |
| Availability/context | Q210 is P0 quality failure; P1/P2 positive passes. Other39 expected guards/abstentions pass. | No cases; no claim | No cases; no claim |
| Edge | Q287 is P0 quality failure; P1/P2 passes. Other bounds/protection fixtures pass. | No cases; no claim | No conditional positive; guard fixtures pass, e.g.Q281 |
| Protected | No guard failure among 44; Q001 representative | No guard failure among 23; Q041 representative | No guard failure among 16; Q021 representative |
| Unsafe/unsupported | No guard failure among 10; Q251 representative | No cases; no claim | No guard failure among 20; Q241 representative |

This table reports actual failure opportunities and zero cells; it does not manufacture failures for guarded families with no answerable target. Exact per-family/split failure IDs are in the saved summaries. Curated synonyms/labels, known fixture heads, tiny dependent groups and absent initial targets limit generalization. Complete synthetic contexts do not solve the 43 unclassified global grammars/default-like admission gap. P2 remains a conditional reference choice; SEARCH-F002 must decide a concrete usable consumer/compatibility boundary before conditional SEARCH-F001 infrastructure is recommended for execution. No ranker/UI implementation is authorized by these examples.
