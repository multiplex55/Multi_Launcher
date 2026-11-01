# Deterministic discovery ranking — M1-B specification

Status: **PROVISIONAL_RESEARCH_DESIGN**. No ranker or evaluator has run, and no production ranking change is accepted. The three policies are predeclared before a corpus freeze, using [candidate_policies.json](evaluation/candidate_policies.json), [catalog.json](catalog/catalog.json) and [grammar/admission](INTENT_TAXONOMY_AND_GRAMMAR.md). Current production fuzzy/provider/usage behavior is described in [CURRENT_SEARCH_ARCHITECTURE](CURRENT_SEARCH_ARCHITECTURE.md); P0 is a transparent lexical **surrogate**, not a reproduction of it.

## 1. Shared inputs, identities and hard constraints

Inputs are admitted normalized text, a current immutable descriptor projection, current owner eligibility/policy stamp, candidate ID P0/P1/P2 and scope subset (full 15 or initial 11). All candidates use the same raw admission, canonical-outcome gates, source identities, exact Actions, subset and unsupported rules. A rejected query produces no discovery result; it is never passed to a more permissive candidate.

First remove missing, ambiguous-name, non-builtin, disabled, capability-denied, stale or malformed descriptor owners. Snapshot enablement, caller policy and declared/configured search remain separate checks. The proposed stronger discovery checks are not existing universal root-row guarantees. Preserve exact source opener action/args; every current descriptor has None args and no inferred payload. Complete reserved-route metadata remains a UI prerequisite, separate from descriptor projection.

Ranking vocabulary is English ASCII. Use the grammar's normalization, deduplicated ordered phrase and sorted unique query set Q. m=|Q| is 1–12 after all bounds. Repeated words have no score advantage. Metadata descriptions and full executable action strings are deliberately **not ranking fields**: descriptions mix long safety explanations and provider styles, and opaque wire payloads must not become keywords. Existing shortcut **labels**, never shortcut wire strings, are searchable fields.

| Candidate | Phrase fields used for exact bonus | Lexical document-token union | Additional signals |
|---|---|---|---|
| P0 | Normalized label + existing shortcut labels | Tokens from those phrases | Exact token and bounded last-token prefix only; no curated aliases, roles, typos or usage. |
| P1 | P0 + proposed aliases | Tokens from all those phrases | Curated source-reviewed vocabulary hypothesis; same lexical matching. |
| P2 | Same phrase fields as P1 | P1 tokens + vocabulary for this entry's finite intent/object memberships | Role coverage/conflict and at most one one-edit corrected query token per descriptor. |

Alias/role lists are completely enumerated in the policy/catalog files. No runtime synonym expansion or external dictionary. Metadata phrases retain first occurrence of repeated normalized tokens; phrase equality preserves remaining token order. Token union is a set. An alias cannot gain multiple bonuses, and duplicating metadata has no effect. Explanations identify fields/tokens, not untrusted metadata as executable instructions.

## 2. Bounded lexical matching, precedence and typo definition

For each descriptor d and each q in Q, choose the strongest available evidence against its document tokens D(d):

1. **Exact:** q equals a token → h(q,d)=100, matched target q.
2. **Prefix:** only the **last content token in the normalized ordered query phrase** may use this rule; q length≥4, a document token starts with q, and target length is at most |q|+4. h=70. Select ASCII-smallest target if several qualify. Other query tokens receive no prefix credit.
3. **P2 typo only:** for a token with neither exact nor allowed prefix evidence, both q and target lengths are 5–24; accept one insertion, deletion, substitution or one adjacent transposition. These are exact distance-one cases, not unrestricted Damerau/Levenshtein search. h=55. q and target equal is already Exact. No two edits, phonetics, split/merged-word or prefix+typo combination.
4. Otherwise h=0. q is unmatched.

P2 may correct **one query token per descriptor**. Examine otherwise-unmatched eligible q tokens in ASCII order; choose the first with any distance-one target, then its ASCII-smallest matching target. All other typo-only opportunities remain unmatched. This deterministic choice deliberately need not maximize semantic relevance; document failures rather than try all correction combinations. Exact/prefix evidence always wins, and one target token can support several unique query tokens. Role conflict/coverage discourages generic partial matches; there is no token-assignment solver.

Define H=Σ h(q,d), **C=floor(H/m)** in [0,100], and U=number of h=0 tokens. Define E=1 if the normalized deduplicated ordered query phrase equals any permitted phrase field; otherwise 0. Phrase equality includes all non-stopword content. Prefix/typo never creates E. Scores use integers and floor division throughout, never floating equality or locale/hash iteration.

Prefix applies only to the final content token to avoid treating every incomplete word as independent evidence. For example, `file checks` may prefix-match `checksum`, while `checks file` cannot prefix-correct its first token. This asymmetry is declared policy, a future corpus case and a limitation; it must not be lost in the prototype. Protected explicit prefixes are already excluded before this rule and are never fuzzy-routed.

## 3. Exact formulas

P0 and P1 use the same formula over different fields:

```text
S01(d) = clamp_0_100(floor(4*C/5) + 20*E - 10*U)
```

P2 computes role coverage, using the complete finite dictionaries in the policy file. Begin QI/QO as the sets of intent/object codes recognized by **exact normalized query tokens**. Add codes recognized by the chosen prefix/typo target tokens for this descriptor. Ordinary exact token targets equal query tokens and add nothing new. These sets can therefore differ by descriptor when spelling evidence differs. Do not map arbitrary near-matches through every dictionary entry.

Let DI/DO be this descriptor's intent/object memberships. Define:

```text
I = floor(100 * |QI intersect DI| / |QI|), or 0 if QI is empty
O = floor(100 * |QO intersect DO| / |QO|), or 0 if QO is empty
JI = 1 if QI is nonempty and QI intersect DI is empty, otherwise 0
JO = 1 if QO is nonempty and QO intersect DO is empty, otherwise 0

S2(d) = clamp_0_100(
    floor(3*C/5) + 15*E + floor(15*I/100) + floor(15*O/100)
    - 8*U - 25*JI - 30*JO)
```

I/O reward coverage of recognized roles, not a calibrated intent estimate. Multi-role requests receive fractional credit if only some recognized roles are supported; an entirely conflicting recognized object is penalized more than a conflicting intent. Unrecognized words still lower C and increase U. “Tool supports one of these words” is not “tool performs the whole instruction.” Common unsupported gates and fixed-opener-only authority remain stronger than all scores.

Rationale is provisional: exact phrase bonus separates curated expressions from accidental token unions; unmatched penalties suppress long partially matched prose; conservative prefix/typo strengths prevent spelling evidence equaling exact text; object/intent roles address opaque opener labels such as XML. Coefficients were chosen before a frozen dataset and **not tuned on any held-out corpus**. A three-policy comparison can reject P2 or retain no shipping ranker; model-generated examples are not independent evidence.

## 4. Deduplication, order, top-k and abstention

After scoring all eligible entries, collapse records with identical **exact `Action.action` bytes and optional `Action.args` bytes**, None distinct from Some(empty). The highest score wins, then smallest descriptor ID; preserve contributing IDs only in explanation. This dedup key does not use labels, descriptions, normalized wire data, decoded-path equivalence or usage key. Current 15 records have distinct action/args keys. A future same-key group with different trusted owner epochs/origins or conflicting operation semantics is a projection validation error: exclude the entire group rather than choose execution provenance by score.

Saved target identity and descriptor identity remain different: two different file targets must not collapse merely because labels or discovery objects look alike; this initial catalog has no per-file targets at all. Args variants with the same action string remain different payloads even though current production usage combines their counts.

Sort deduplicated candidates by **score descending, synthetic descriptor ID ascending using ASCII byte order**. There is no arrival-order/label tie breaker. Let qualified candidates have score≥**60**. Retain their first **5** for possible display. Determine result state from the first two qualified scores after dedup:

- None: `ABSTAIN_NO_MATCH`, no executable row.
- Top score≥**75**, and either no second qualified candidate or top−second≥**12**: `SUGGEST_OPEN_TOOL_NOT_ACTIVATE`. Show up to five qualified rows, with the first called the stronger suggestion; explicit user selection is still required.
- At least two qualified candidates and clear-single condition fails: `CHOOSER_EXPLICIT_SELECTION`, no implied default operation.
- Exactly one qualified candidate with score60–74: `ABSTAIN_LOW_EVIDENCE`, no executable row. Do not convert a lone weak typo into a confident result.

A near tie is a strict gap<12. Gap=12 is clear when the top meets75. Top-k is presentation capacity, not a second filtering threshold or authority grant. If the selected row is no longer current/eligible, invalidate it rather than substitute the new first row; runtime binding owns this, not the tie rule.

## 5. Hand-worked decompositions — no ranker execution

These arithmetic examples assume Complete/current admission and eligible owners solely to illustrate formulas. They do **not** establish that default live configuration can reach discovery. E, role membership and matching targets can be checked against the draft files. No dataset labels or result metrics are claimed.

| Query / candidate | Hand calculation | Consequence / counterexample |
|---|---|---|
| `open checksum tool` / Hash | Stopwords leave `checksum`. P0 label/shortcut has no hit: C0,U1,E0→0. P1 exact alias: C100,U0,E1→100. P2 also I100/O100: 60+15+15+15=105→100. | A plausible lexical-alias gain, not actual launcher baseline superiority. Requires future live coverage/admission preconditions. |
| `make json readable` / JSON format | P0 only json matches: H100,m3,C33,U2,E0→26−20=6. P1 alias equals phrase: C100,E1→100. P2 recognizes format/json, E1→105→100. | General JSON/minify share some object terms but do not inherit format intent. Source Action opens format mode; it does not transform this sentence. |
| `format xml` / XML | P0/P1 hit xml only: C50,U1,E0→40−10=30. P2 role vocabulary supplies format+xml: C100,E0,I100,O100→60+15+15=90. | Role metadata can recover a source-verified tool function absent from short aliases. It is still an opener, not XML execution. |
| `compare` / Image Compare and empty Diff | P1 both have compare in aliases, C100,E0→80 each. P2 both recognize compare intent, no object: C100,I100,O0,E0→75 each. | Both policies return a chooser; stable ID order resolves display tie only. Initial-11 subset lacks Diff and must report this absent target, not silently drop the case. |
| `file cheksum` / Hash, P2 | file exact100; cheksum→checksum one insertion55; C=floor155/2=77,U0,E0. Roles after correction are compute_hash and file/hash, all covered: 46+15+15=76. | A bounded typo can qualify with context. P1 has only file: C50,U1→30. |
| `cheksum` / Hash, P2 | C55,E0,I100,O100,U0→33+15+15=63. | A sole score63 is low-evidence abstention, despite a known correction. No aggressive one-word typo promise. |
| `file checks` / Hash, P2 | Last-token checks→checksum prefix70, file exact100: C85,E0,I100,O100→51+30=81. | `checks file` cannot use prefix on its first content token and can abstain. Predeclared prefix asymmetry matters. |
| `checksum checksum` / Hash | Same normalized phrase/set as checksum; score unchanged in every policy. | Raw token count still limits excessive repetition; dedup is not an unbounded-input escape. |
| `quasar bicycle` | No usable field/role match: C0,U2→0. | No-match abstention; generic always-show-tools fallback is prohibited. |
| `delete duplicate files`, `extract zip`, `compare similar images`, `C:\\x\\a.json` | Shared unsupported/protected gate, before any scoring. | No score is calculated/used to recover an opener. Canonical system retains control. |

This table demonstrates calculations and known limitations only. P0 may win precise label cases; P2's broad finite roles may create false neighbors or ambiguous choosers. Description omission can lose legitimate relevance. Unsupported-verb protection can reject benign prose, and English punctuation/Unicode restrictions reduce coverage. M1-C must include those failures without rewriting weights to match them.

## 6. Personalization, complexity and deterministic limits

Initial shipping and all three prototype policies use **zero personalization**. They read no usage/history and add no persisted fields. A later sensitivity check can assign synthetic counts0 versus1,000,000 and require identical results; it is an invariance check, not a fourth candidate or optional usage ranking mode. Current production action-string/count weighting remains untouched. No recency, TTL, history-derived labels or time-of-day behavior is proposed.

Descriptor IDs match `[a-z][a-z0-9_.-]{0,63}` (at most 64 ASCII bytes). Projection validation caps **64 descriptors**, 8 aliases/descriptor, 96 UTF-8 bytes and 8 normalized tokens/alias, 8 shortcut labels/descriptor, 128 unique lexical document tokens/descriptor and128KiB serialized projection metadata. Current 15/11 research scopes fit by static inspection; this is not a production memory measurement. Oversized/invalid projection fails closed as a whole, with diagnostic counts only; do not silently truncate owners or aliases and claim complete coverage.

P2 constructs eligible potential distance-one pairs only for otherwise-unmatched tokens of permitted lengths, in defined query/document ASCII order. If the total potential pairs across all descriptors exceeds **4096**, return `ABSTAIN_BUDGET` for the whole query before typo-dependent results publish; no prefix of the catalog receives privileged work. One-edit tests can be implemented in O(L), L≤24, by length check and a single mismatch scan (plus adjacent-transposition check). No dynamic-programming matrix or combinatorial correction search is required.

With N≤64, Q≤12, D≤128 and L≤24, direct exact/prefix comparison is O(NQD·L) and bounded typo work O(4096·L); sorting is O(N log N), transient score state O(N+QD). A prepared set/index may reduce comparisons without changing tie/match semantics; a new crate, worker or persistent index is unnecessary at 11/15 entries. Exact serialized metadata bytes, transient allocations, existing app memory and strings' allocator overhead are different quantities. Runtime targets are unmeasured and separately stated in the integration design.

## 7. Evaluation/version and acceptance boundary

`g02-m1-b-v1` declares exactly P0/P1/P2. There is no evaluator or prototype at M1-B. M1-C must freeze catalog, role/alias memberships, numeric policy, family split, labels and protocol with explicit canonical-content hashes. Any later formula defect fix consumes the allowed correction cycle and is logged; no post-holdout policy shopping. Compare both subsets using identical contexts/cases/labels and report conditional discovery quality, gate exclusions, absent targets, abstention/chooser and forbidden-suggestion denominators separately.

M1-C freezes these exact definitions under `g02-eval-v1`; numerical policies remain provisional production-design candidates, not accepted production recommendations. M2 evidence may justify a pure-ranker package, a narrower design or no change. M3-A must provide complete role/canonical pairs for anything actually accepted; SEARCH-F001 alone supplies metadata and changes none of these scores/UI consumers.
