# Goal02 substantive review

Reviewed artifact commit: **3a4924e20a95de38af3ba60aca6b86ade1e1c0f6**. Application source: **59156d96adcc73939c7171487a0cb582e3286587**. Goal02 base/closed Goal01: **3b79d73f12a791b8b077ed0bf7d6631bd885b68c**.

**Disposition: accept the research with its stated limitations after one bounded documentation amendment below.** No production search change is justified by this review. P2 remains a conditional reference; SEARCH-F002 remains the first proposed search decision spike, with SEARCH-F001 conditional on explicit acceptance of a concrete useful consumer/compatibility contract and separate authorization. No new package is required.

This is one separate-context substantive review, using a reused reviewer context. It is not blind validation of the corpus, a second whole-repository audit or proof against shared assumptions. [Role record](ROLE_RECORD.md) describes actual checks and exclusions.

## Required correction, ordered by severity

### G02-REVIEW-01 — Medium: F002's general network/subprocess prohibition conflicts with its execution contract

**Affected locations at the reviewed commit:**

- Canonical [F002 Goal](../../follow-on/SEARCH-F002-discovery-admission-consumer-proof/GOAL.md), section 3, line 44, broadly prohibits `network/subprocess` activity.
- Canonical [F002 Kickstart](../../follow-on/SEARCH-F002-discovery-admission-consumer-proof/KICKSTART.md), section 4, line 25, repeats that broad prohibition.
- The same pair explicitly requires branch/checkpoint Git operations and one normal push after each checkpoint: Goal section 7, particularly line 131; Kickstart sections 2, 4 and 6.
- Goal section 4, line 82, and Kickstart section 4, line 33, permit a bounded direct Python invocation. Goal invariant 9 also broadly excludes automatic external writes.

**Why this matters:** a future executor following the package literally cannot both honor the general network prohibition and perform its required normal push. An undifferentiated subprocess prohibition also obscures the distinction between ordinary orchestration tools/direct Python invocation and forbidden subprocess spawning inside the optional proof. This is an execution-scope contradiction, not a request for broader experiments or network access during current research.

**Precise amendment to the existing canonical pair:**

1. Remove the unqualified `network/subprocess` terms from the general prohibited-activity lists in Goal section 3 and Kickstart section 4. Preserve all other exclusions.
2. Add the following scope clarification to both locations:

   > After this package is separately authorized, ordinary local source/document tools, read-only Git inspection, task-branch/checkpoint Git operations and the specified single normal non-force push per checkpoint are permitted. Git publication is the only permitted network operation; a failed push is recorded and local work continues without a recovery loop. The explicitly bounded Python proof may be launched directly. The proof itself must perform no network access or subprocess spawning, and must not invoke providers, application/native code or dependency installation. No other external write is authorized.

3. Make the optional-proof sentence explicitly refer to prohibitions **inside the proof**, retaining its existing 128-state, two-invocation, 30-second-per-run and 60-second-aggregate caps.
4. Qualify Goal invariant 9's external-write prohibition with the same narrow exception for specified task Git publication. Keep future goals, production implementation, PR creation, merge, release and unrelated external actions excluded.
5. Record this canonical clarification in [CANONICAL_DESIGN_AMENDMENTS.md](../../CANONICAL_DESIGN_AMENDMENTS.md) and the parent correction receipt. Adjust the canonical pair/index statements that currently say canonical changes are “navigation/provenance only” to distinguish original M3-A promotion from this M3-B clarification. Preserve raw author files and the historical identities in [M3A_PROMOTION_RECORD.json](../../M3A_PROMOTION_RECORD.json); any new canonical hashes must be clearly identified as post-review identities rather than replacing historical evidence.

**Acceptance check:** inspect the affected document diff and confirm both companions agree. No prototype, network experiment, source change or new package is needed. This amendment grants no permission to execute F002 during the current research.

## Findings corroborated without further amendments

### Current architecture and capability ownership

The [architecture](../../CURRENT_SEARCH_ARCHITECTURE.md) retains the closed audit's consequential distinctions: explicit `g` early return, fallback only for a non-`g` empty input-provider vector, no fallback retry after exact filtering, capacity composition without fallback, and separate failed-provider reopening. It distinguishes matching-prefix Literal from the default ActionFilters path and does not treat snippet-local preservation as end-to-end transport preservation.

Manager metadata already supplies immutable origin/name/epoch/revision/enabled/declared-capability facts without query-time provider locks. Configured capability restrictions remain separate; the current manager admission function does not itself require the declared capability string. The proposed stronger discovery eligibility is correctly labeled new policy. See [snapshot ownership](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1108-L1159) and [current admission](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1310-L1437).

The eleven initial records match source-owned fixed openers across nine plugins, including distinct JSON intents and Table/CSV profiles. Query shortcuts remain distinct from direct opening Actions. The four extra research records remain excluded from F001. File Search Open/SetMode remains separate from StartSearch in the [host](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/command_host.rs#L886-L921) and [dialog](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/file_search_dialog.rs#L698-L731). None of this proves successful native opening, a blank retained session or Diff mutation authority.

Canonical results, completion/cache and automatic consumers remain excluded from the proposed discovery collection. The reports preserve Macro's instantaneous zero/one/many decision and distinguish PresentedForSelection handoff from completed effects. Real ActivationSource/ActionSurface, proposed query origin, exact Action+args identity and selected binding remain separate. Existing settings acknowledgment/reconciliation and worker retirement owners are not replaced by ranker policy.

### Algorithm, saved measurements and correction integrity

Static reading of [reference.py](../../prototype/reference.py), [evaluate.py](../../prototype/evaluate.py) and the [frozen specification](../../RANKING_SPECIFICATION.md) found no material transcription discrepancy affecting the published comparison. Relevant mechanics include final-content-token prefix matching, one deterministic distance-one correction per descriptor, integer floors/conflict penalties, exact Action+args deduplication, score/ID ordering, 60/75/12 result thresholds and zero personalization.

Independent saved-data arithmetic reproduced:

| Scope | P0 top1/top5 | P1 top1/top5 | P2 top1/top5 | P2 chooser |
|---|---:|---:|---:|---:|
| Full 15 | 8/119; 8/119 | 50/119; 51/119 | 71/119; 73/119 | 7/30 |
| Initial 11 | 7/119; 7/119 | 39/119; 39/119 | 58/119; 58/119 | 3/30 |

The same pass reproduced the published MRR/nDCG values, 181 desired abstentions and zero forbidden outputs for every policy/scope. Holdout hits are P0=1/18, P1=5/18, P2=6/18 in both scopes; Q026 is P2's sole additional holdout hit. The relative P2 preference follows the frozen criteria. It does not establish strong absolute quality, human preference or superiority over current Rust search.

Only Q122's expected labels, rationale/provenance/confidence and source references differ between v1 and v2. Query, context, identity and split are unchanged; all other cases are unchanged. [DataPlugin](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/data.rs#L25-L68) corroborates the already-reserved `data` head. The correction is a defensible label-integrity amendment, unlike relabeling a semantic miss to improve agreement.

All **1,800** original/corrective result objects compared equal. Static inspection confirms the correction script reuses predictions and recomputes observations rather than invoking query/ranking functions. Original 120-positive/163-gated evidence remains separate from amended 119/164 evidence. No correction allowance remains.

No split group crossed splits, and no identical trimmed/case-folded query crossed splits in the checked corpus. This does not eliminate semantic overlap or author visibility. The reports correctly disclose those limits, the small XML/comparison holdout, missing split/family cells, initial-subset absent targets and unchanged denominators. Q132's regression, Q138's wrong clear suggestion, weak chooser results and contestable semantic labels remain visible.

### Admission and package readiness

The registration/default-policy source and sampled global guards support the distinction between an 81-registration declaration inventory and a completed grammar manifest. Web/Calculator have restricted bodies despite global metadata; Snippets' adjacent-prefix behavior defeats a declared-head-only assumption. This review did not classify the remaining 43 global bodies. The reports correctly leave Complete/default-like usefulness unresolved, including disabled owners and opaque origins.

Both canonical F001 companions require an explicitly accepted concrete useful consumer/compatibility contract, its version/F002 closing evidence and separate F001 authorization. Source classification or hypothetical Complete fixtures alone cannot satisfy that gate. F002 permits a bounded no-change outcome and does not force a grammar framework or runtime campaign.

The canonical owner `src/search_discovery`, initial eleven-descriptor scope, 64-byte annotation limits and strict 32-KiB text accounting are separated from the frozen experimental `src/discovery` spelling and wider envelope. Constructor extraction, immutable projection, stale eligibility rejection, focused future tests/lint and rollback are concrete. No current metadata implementation is recommended merely because the prototype has conditional gains.

The per-goal and program catalogs consistently retain four proposed packages: independent AUD-F001/AUD-F002, independent SEARCH-F002 and conditional SEARCH-F001. No ranker/UI/personalization/corpus-service package or queued worker experiment is silently accepted. Raw proposals remain historical; canonical amendments explain supersession.

## Limits and closeout boundary

No production defect beyond the already closed audit findings is established by this review. No application, Rust/native test, provider, live data, dependency or network action ran. The saved prototype was not imported or invoked. Static arithmetic checks are not new relevance experiments.

The empty inspected production-path diff and unchanged Goal01 subtree support the research-only boundary. Parent mechanical checks complement this semantic review; their counts and receipts remain parent evidence. External comparisons were assessed through saved official-reference reports, without a new network verification or product trial.

Current research can close with its stated limitations after G02-REVIEW-01 is addressed. Native/runtime correctness, default-like discovery reachability, human relevance and a real consumer remain unproved; these are explicit limits supporting the no-production-change conclusion, not reasons to expand this review.

The parent owns the single correction pass, affected document checks and final acceptance receipt. This reviewer has not examined post-review amended text. No second review campaign, new experiment or future implementation is requested.
