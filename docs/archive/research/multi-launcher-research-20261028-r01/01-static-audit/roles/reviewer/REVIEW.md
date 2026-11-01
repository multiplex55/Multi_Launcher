# Goal 01 — critical research review

Reviewed artifact commit: `99b07204fdc5d1b78019139f6c8a6613e025bd30`.
Application source: `59156d96adcc73939c7171487a0cb582e3286587`.
Reviewer: `/root/research_reviewer`, separate context; requested/configured Astra max, resolved backend/effort unverified. See [ROLE_RECORD.md](ROLE_RECORD.md).

**Disposition: accept after the two factual document amendments below and the already-planned M3-B closeout.** Neither amendment requests production changes or a new future package. The source-backed tokenizer correction and bounded Diff decision spike remain justified in their current forms. No broader refactor, resource investigation or native campaign is warranted by this review.

This disposition is conditional: the child reviewed the stable pre-correction artifacts, not the parent's eventual amended text. The parent should record the applied corrections and final disposition during the single planned correction pass.

## Findings ordered by severity

### REVIEW-01 — Medium: the search handoff overgeneralizes command fallback

**Affected reports:** [SEARCH_HANDOFF.md](../../SEARCH_HANDOFF.md), sections 2–4, especially original lines 28, 44 and 50; [ARCHITECTURE_AND_DATA_FLOWS.md](../../ARCHITECTURE_AND_DATA_FLOWS.md), the query-routing/scoring tables at original lines 199–200 and 215.

The reports say an empty provider result produces command-cache fallback and that occupied provider capacity can compute “local/fallback” rows. These descriptions combine different branches of the current implementation.

Source establishes:

1. `search_plugins_from_results` handles `g ` first and returns from that branch before reaching command-cache fallback. An empty web-provider vector therefore does not trigger command fallback.
2. For other routes, fallback is selected when the input provider vector is empty. It is not retried merely because exact post-filtering subsequently removes every action from a nonempty provider vector.
3. `search_read_only_outcome` handles a nonempty query with occupied provider capacity by passing an empty **already-scored** provider vector directly to `search_read_only_outcome_from_scored_plugins`. This bypasses `search_plugins_from_results` and its command fallback. Local `app` scoring can still contribute rows; other routes have no rows, with Pending/Capacity retained.
4. The failed-provider reopening helper, `search_read_only_outcome_without_providers`, explicitly invokes `search_plugins_from_results`. Its possible non-`g` command fallback is a separate path from capacity deferral.

Evidence: [g early return and fallback admission](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1176-L1248), [capacity branch](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1395-L1434), [failed-provider reopening](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1456-L1473), and [local/scored-provider assembly](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1579-L1622). The existing `manual_query_fallback_does_not_reenter_a_blocked_provider` test also asserts Pending with empty rows for its occupied-capacity ordinary query; it was read, not run.

**Consequence:** Goal 02 could incorrectly model command suggestions as available during capacity deferral or an empty explicit web search. That changes candidate recall and can change the Macro handler's instantaneous zero/one/many outcome. The issue is an inaccurate compatibility baseline, not a newly identified production defect.

**Precise amendment:**

- In both reports, replace the unrestricted empty-provider fallback row with:

  > For a non-`g` query, an empty provider-return vector enters command-cache fallback under the current exact/fuzzy rules. The explicit `g ` branch returns without that fallback. A nonempty provider vector that becomes empty during exact post-filtering does not trigger another fallback pass.

- In both scoring tables, qualify the command-fallback row with that same admission condition. Do not imply that fallback occurs whenever the final displayed provider result set is empty.

- Replace the architecture table's occupied-capacity description with:

  > For a nonempty query, occupied provider capacity skips provider execution and command-cache fallback. The assembly receives no scored provider rows; eligible local `app` rows can remain, while other routes are row-empty. The result retains `Capacity` deferral and `Pending`. Empty browse is handled before this branch.

- Replace the handoff's sentence “Capacity deferral can compute local/fallback rows while pending” with:

  > Capacity deferral can retain locally scored `app` rows while pending, but does not run command-cache fallback; other nonempty routes are row-empty. Failed-provider reopening is a separate path that explicitly calls the provider-result scorer and can use its non-`g` command fallback.

Keep the existing claims about current-query refresh, pending versus finality, and Macro instantaneous-cardinality behavior. They are supported.

**Mapping:** existing architecture report and SEARCH_HANDOFF only. No new follow-on pair or production test request.

### REVIEW-02 — Low: the snippet no-tag counterguard is local, not an end-to-end exemption

**Affected material:** [AUD-001](../../findings/AUD-001-terminal-query-escape.md), particularly original lines 17 and 34; D01 in [DECISION_RECORDS.md](../../DECISION_RECORDS.md); the AUD-F001 row in [RECOMMENDATIONS.md](../../RECOMMENDATIONS.md); the token paragraph in [SEARCH_HANDOFF.md](../../SEARCH_HANDOFF.md); and the counterguard explanation in the canonical [AUD-F001 Goal](../../follow-on/AUD-F001-preserve-terminal-query-escape/GOAL.md).

The canonical Goal and Kickstart already say the snippet no-tag branch preserves input **locally**, which is correct. Some summaries shorten this to “no-tag snippet routes limit impact” or a “no-tag exception.” The finding calls it counterevidence to every snippet query being affected. That wording can incorrectly imply that omitting tags protects a query throughout the normal launcher path.

`SnippetsPlugin` inherits `PluginQueryPolicy::ActionFilters`. Normal and snapshot manager search reconstruct query text through `split_action_filters` before calling the provider. For a manager input ending in one unconsumed backslash, that upstream reconstruction can already add the extra backslash. `parse_snippet_query_filters` returning its no-tag argument unchanged preserves the already-transformed argument; it does not restore the original launcher input.

Evidence: [default provider policy](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L104-L143), [normal manager reconstruction and query plan](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1331-L1390), [snapshot reconstruction](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L535-L577), [snippet parser](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/snippets.rs#L407-L445), and [snippet provider entry points](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/snippets.rs#L747-L887). A source search found no `query_policy` override in this provider.

**Consequence:** the shortened wording understates the affected transport path and can cause future search research to classify untagged snippet inputs as unaffected baseline controls. It does not invalidate the tokenizer fix or its existing test plan.

**Precise amendment:**

Use this boundary explanation in the finding and canonical Goal:

> Matching-prefix Literal providers receive the original manager input. Separately, `parse_snippet_query_filters` returns its own argument unchanged when no tag token is present; this avoids reconstruction inside that helper only. `SnippetsPlugin` uses the default ActionFilters policy, so normal and snapshot manager paths can already have altered an untagged snippet query before the helper sees it. The no-tag direct-parser control is therefore not an end-to-end exemption from AUD-001.

In D01, RECOMMENDATIONS and SEARCH_HANDOFF, replace shorthand “no-tag exception” or “no-tag routes limit impact” with “snippet-local no-tag preservation; upstream manager reconstruction still applies.”

The canonical Kickstart's local qualification is already accurate; append the upstream-manager distinction to section 1 so the companion cannot be read as promising an end-to-end exemption. Keep its direct-parser no-tag control, inert manager tests, source scope, low severity and unmeasured runtime impact unchanged.

Record this clarification in FOLLOW_ON_INDEX's canonical amendment history. Preserve the raw planner proposal files.

**Mapping:** precise clarification to existing AUD-F001 and its reports; no duplicate package, expanded implementation or additional test campaign.

## Substantive conclusions retained

- **AUD-F001 is correctly owned and implementable.** The scanning branch records the backslash in `raw`; pending-escape EOF records it again. Completing only the deferred decoded value is the narrow correction. The proposed tests distinguish raw spelling from decoded semantics, preserve filter grammar and inter-token normalization, exercise normal/snapshot provider transport, and use available inline inert seams. Matching-prefix Literal handling is a real manager-level counterguard. No parser rewrite, new dependency or native proof is needed.

- **AUD-F002 is appropriately a decision/proof spike.** Diff's inspected Windows identity comparison is metadata/path based, and `fs::copy`/trash operate by path after validation. GUI captured-plan handling, dirty-path checks, containment, recycle-only admission and no permanent fallback are real counterguards. The existing native held-file owner is a distinct stronger mechanism, not an automatic Diff replacement. The package separates source, destination, ancestor, content and object identity; permits a no-change outcome; bounds the optional model; and does not convert an abstract witness into native proof. No production repair has been established or authorized.

- **Lifecycle distinctions are supported by the sampled owners.** File Search can publish cancellation before its detached worker returns. Duplicate Finder's reopen gate depends on the reaper's post-join receipt; Image Compare uses its worker teardown signal and retained budget. The reaper calls its callback after join and has an explicit failure path. These observations support the report's rejection of a blanket retirement guarantee or generic scheduler recommendation.

- **Settings already has the described protocol.** `run_settings_save` prepares a ticket, merges owned fields against latest persisted settings, and reconciles authoritative runtime disposition. `NotEnqueued` differs from an accepted apply request; timeout is not permission for blind rollback. `rollback_after_failure` uses owned-field conflict checking. No missing global save coordinator was established.

- **The remaining sampled search constraints are supported.** Usage is keyed by action string; pins distinguish action plus args. Search clears selection even on cached/equal-result paths. Pending can coexist with rows outside the specific capacity branch corrected above. The production Macro command broker uses instantaneous zero/one/many and accepts PresentedForSelection as handoff success; the legacy exported/polled query broker has a different first-row/Enter contract. These are compatibility facts, not reasons to merge the brokers.

- **The package boundaries remain coherent.** AUD-F001 has one production owner, directly related tests and scoped future verification. AUD-F002 authorizes only documentation/optional pure model, with no native or production execution. Both revalidate a future accepted base, preserve current defaults and ownership, specify rollback/failure handling, and remain PROPOSED_NOT_AUTHORIZED. Neither depends on Goal 02 or the other package.

## Blocking corrections versus nonblocking limits

The two findings above are factual amendments required before treating this version of the search handoff as final. They are bounded documentation work inside the existing reports/package. They do not require a new research stage, production repair, additional child review or native evidence.

Routine M3-B closeout still needs to persist this review, apply and record the amendments, finalize README/REVIEW_AND_LIMITATIONS and progress, and record the closing commit. These were planned deliverables; their pre-review pending state is not a newly discovered implementation defect.

The following remain nonblocking research limitations:

- Coverage is explicitly risk-weighted: the matrix reports 11 bounded deep, 32 targeted and 62 survey-only buckets. A deep bucket does not certify every file within it; planned historical depth is not automatically achieved coverage.
- Native focus, file replacement, trash behavior, blocked backend timing, crash/power-loss behavior and actual resource/performance costs were not executed or measured.
- Existing tests were inspected as source. Model-only fixtures, native gates and uninspected branches cannot become passing runtime evidence.
- This reviewer did not repeat the entire source inventory, every provider/dialog trace or the parent's mechanical link checks.
- Requested/configured models do not prove resolved backend identity. Separate-context review does not remove shared assumptions.
- Parent closeout edits made after the reviewed commit are outside this frozen review until the parent records its correction disposition.

No additional substantive issue was found in the bounded review. Dashboard/Macro redesign remains NO_ACTION; worker-bound speculation remains conditional watch without queued work. There are still exactly two proposed follow-on packages.
