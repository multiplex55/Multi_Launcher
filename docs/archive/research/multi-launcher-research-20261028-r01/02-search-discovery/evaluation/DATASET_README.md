# Frozen synthetic corpus — v1

M1-C authoring, 2026-10-28. **300 model-authored cases; all labels visible to the ranker author; not blind, human ground truth, live user distribution or a current-Rust route replay.** No evaluator/ranker code or execution occurred during creation. [Protocol](EVALUATION_PROTOCOL.md), [cases](queries.jsonl), [splits](splits.json), [selected route fixtures](route_fixtures.json), [policies](candidate_policies.json) and [freeze manifest](freeze_manifest.json) define the experiment.

## Coverage and counts

| Family | Cases | Coverage reason |
|---|---:|---|
| protected | 83 | Explicit real owner heads, app/g/Omni, Literal/filter/quote/escape, paths/URLs/shell/process syntax and payload-looking strings. |
| natural | 55 | Fifteen real fixed openers, general/mode intent distinctions and description-led challenges; no invented capability. |
| typo | 32 | Missing/extra/substituted/transposed letters, short-object cutoff, incomplete final token and non-final prefix counterexample. |
| ambiguity | 40 | Untyped comparison, structured-data tool/profile overlap, search modes and broad intent. Multi-answer grades avoid arbitrary unique labels. |
| availability | 40 | User/programmatic/automatic source, real surfaces, pending/deferral/stamps, settings transition, origin/epoch/capability/disable and selected-before-change fixtures. |
| unsafe | 30 | Missing ZIP/history/full-export/semantic-image functionality, destructive/unattended/clipboard/privacy-sensitive requests with no inferred operation. |
| edge | 20 | Empty/short/digit/repeated/oversized inputs, raw/content token caps, Unicode/control/locale/whitespace. |
| **Total** | **300** | Distinct cases include intentionally repeated text under materially different authority/state guards. |

The initial suggested 70/80/40/35/30/25/20 allocation was adjusted before evaluation. Fifteen fixed openers do not justify 80 padded natural paraphrases; protected syntax, eligibility and selection-state risks require more coverage. Repeated natural phrases in state fixtures exercise different hard boundaries, not independent relevance samples. Family describes the challenge; it does **not** imply admitted discovery. For example, natural-looking `xml viewer` begins a real reserved head and is labeled protected.

Frozen expected gate labels:103 ProtectedSyntax,137 ADMIT,11 InputBound,2 Disabled,12 ExcludedSurfaceOrOrigin,11 BusyOrStale,3 CoverageUnknown,1 NoEligibleOwner,20 UnsupportedIntent. There are120 conditional answerable positives (ADMIT with some grade≥2),17 admitted negatives and163 gated cases. These are **labels/counts**, not observed candidate successes or actual Rust routing. Full 15 and initial 11 use exactly these same cases/labels/denominators; absent targets remain.

## Splits and leakage limits

| Split | Cases | Share | Conditional positives |
|---|---:|---:|---:|
| development | 173 | 57.7% | 68 |
| validation | 61 | 20.3% | 34 |
| holdout | 66 | 22.0% | 18 |

Whole semantic groups were explicitly assigned before evaluation, with no row shuffle or random seed. Actual membership/group map is authoritative in splits.json. Owner groups keep clean phrases, typos, mixed-width/locale variants, state/eligibility copies and quoted/payload variants together. Broad File Search variants merge into the same search group; untyped comparison is a distinct ambiguity group. Holdout reserves that comparison task, XML owner challenges, archive/privacy negatives, URL and input-edge groups. Validation contains Table/CSV/Activity/Diff/structured ambiguity/other explicit heads. Whole-group sizes explain the departure from exactly 60/20/20.

The author sees every label, alias and role dictionary and has already hand-worked some design examples. Shared vocabulary/source capabilities are visible across splits; grouping reduces obvious near-template leakage, not all semantic overlap. This is **author-visible synthetic holdout**, not independent validation. No random-user sample, calibrated probability, statistical significance or human preference inference is supported. Do not add new aliases or move cases after unfavorable holdout output.

## Record schema and label semantics

Each JSONL line contains a stable `Q001`–`Q300` ID, exact query string, family, authoring cluster, split_group/split, full synthetic context, expected labels, rationale/provenance/confidence and source_refs. `cluster` can describe the authoring challenge; `split_group` is the merged near-template unit. JSON escapes retain newline/tab/terminal slash payloads; physical line-ending normalization does not rewrite query values.

Context explicitly provides feature/input/settings state, proposed query origin, actual ActionSurface name, session/query/stamp currency, canonical row count/pending/deferral/catalog stability, route coverage/revision, all-owner enablement, owner overrides and synthetic usage count. Default owner facts are present/unique/Builtin/current epoch/enabled/declared-search with no capability restriction. Overrides can set enabled/present/unique/origin/epoch_current/declared_search/capability_restriction. Absent restriction is unrestricted; empty list denies search. all_owners_enabled=false disables every owner. These are fixture inputs, never read from user settings.

Some wrong-origin/availability fixtures deliberately isolate descriptor eligibility behind a hypothetical Complete context. They test defense in depth and are not claimed realizable production configurations: the proposed current UI design would also classify an opaque external registration as Unknown. The route fixture file copies declared heads and reviewed markers only; its status explicitly denies being a complete production manifest. Every Complete context is a **counterfactual prerequisite input**, not resolution of 43 remaining global grammars/default-like reachability. Unknown fixtures must suppress discovery even when the relevant tool is otherwise usable.

`expected.admission` is the proposed policy gate label, not a Rust parser result. `route=CANONICAL_UNCHANGED` means discovery must not intervene; the harness does not execute or predict every existing canonical row. `behavior` distinguishes desired ABSTAIN, CHOOSER or SUGGEST_OR_CHOOSER. `relevance` grades3=direct suitable opener,2=legitimate alternative,1=related but insufficient, missing=0. Grades≥2 define acceptable IDs. A label may be a challenge the predeclared formulas fail; correct that candidate's recommendation, not the label to fit its scores.

`forbidden_ids` forbids emitted discovery rows, especially protected/unsafe/unavailable cases; `forbidden_operations` forbids auto activation, inferred payload, canonical mutation and private-data sampling globally. Binding fixtures record prior selection and INVALIDATE_NO_RETARGET, but represent final current state only. They do not model native input timing or a full GUI state machine. `source_refs` points to actual owner evidence and the research contract; rationale/confidence disclose judgment. High confidence is used for narrow grammar/gate contracts, medium for semantic relevance; no human adjudication occurred.

## Freeze/reproduction

Use the manifest's canonical UTF-8/LF hashing rules; never compare Windows raw working-copy hashes to Git blob hashes without normalization. The manifest excludes itself and mutable progress/role/README outputs. Source hashes/pins preserve provenance; no real application provider, parser, native action, file/clipboard or external service is invoked by dataset preparation or later evaluation. M2-A alone may implement/run the bounded standalone reference prototype after parent publishes this freeze.
