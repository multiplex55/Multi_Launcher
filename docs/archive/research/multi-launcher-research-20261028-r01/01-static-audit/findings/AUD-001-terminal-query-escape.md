# AUD-001 — A pending terminal escape is duplicated in RawToken.raw

State: **Observed source inconsistency**, static only. Category: query correctness. Baseline: `59156d96adcc73939c7171487a0cb582e3286587`. Original proposal: `/root/audit_planner`, configured Astra max, backend unverified; current source/caller countercheck: `/root/audit_author`, accepted Sol xhigh dispatch, backend unverified. Separate review complete; parent applied REVIEW-02 clarification. See [review](../roles/reviewer/REVIEW.md).

M3-A canonical proposal: [AUD-F001 Goal](../follow-on/AUD-F001-preserve-terminal-query-escape/GOAL.md)/[Kickstart](../follow-on/AUD-F001-preserve-terminal-query-escape/KICKSTART.md), PROPOSED_NOT_AUTHORIZED. [Catalog](../FOLLOW_ON_INDEX.md) preserves raw authorship and parent branch-stem amendment; this finding has not been fixed or executed by research.

## Current behavior and owner

`src/common/query.rs::tokenize_query_with_raw` owns decoded token values and their raw spelling. At input backslash it appends the character to `raw` and sets `escape`. If input ends while that flag remains set, EOF handling appends another backslash to `raw` and one to decoded `value`. The latter is needed to represent the dangling escape as a literal character; raw spelling already contains it. [Q1]

For input bytes `78 5C` (x, then one backslash), this branch yields raw spelling `78 5C 5C`. This is a hand-traced source example, **not a Rust execution result**. The violated invariant is token spelling, not byte-for-byte preservation of the entire query: `split_action_filters` intentionally reconstructs remaining tokens using one space. [Q2]

## Callers, concrete mechanism and impact

`split_action_filters` classifies `kind:`/`id:` by decoded token value, then forwards `token.raw` for ordinary tokens. The extra backslash can therefore reach local `app` search text and normal provider queries through this shared boundary. The normal manager and provider snapshot both use the same query plan/filter helper. [Q2], [Q3], [Q4]

Matching-prefix Literal providers receive the original manager input. Separately, `parse_snippet_query_filters` returns its own argument unchanged when no tag token is present; this avoids reconstruction inside that helper only. `SnippetsPlugin` uses the default ActionFilters policy, so normal and snapshot manager paths can already have altered an untagged snippet query before the helper sees it. The no-tag direct-parser control is therefore not an end-to-end exemption from AUD-001. [Q3], [Q4], [Q5], [review source corroboration](../roles/reviewer/REVIEW.md#review-02--low-the-snippet-no-tag-counterguard-is-local-not-an-end-to-end-exemption)

User impact is a possible unexpected search spelling/result mismatch for a token ending in an odd terminal escape. Actual frequency, native path effects and downstream behavior were not measured. No command was executed or filesystem path opened. The finding does not establish that every Windows path, every provider or every activation is corrupted.

## Counterevidence and limits

- A terminal pair of backslashes consumes the second character in the ordinary `escape` branch and clears the flag; it does not enter this EOF branch.
- Matching-prefix `PluginQueryPolicy::Literal` providers receive the original query and skip action filters. QR explicitly uses Literal and its parser preserves the post-prefix payload. [Q3], [Q6]
- Nonterminal escapes and quote handling have separate branches; a repair must retain their established decoded-value grammar.
- Existing `split_action_filters_preserves_query_tokens` covers quoted tokens/filter removal, not a pending terminal escape. The QR sync/snapshot literal test is an existing preservation contract. Tests were read only. [Q7], [Q8]

## Classification and alternatives

| Judgment | Assessment |
|---|---|
| Severity | Low: bounded search correctness inconsistency; no proven destructive execution or data loss. |
| Confidence | High in the source mechanism/raw-spelling invariant; runtime frequency and wider effects unknown. |
| Conditions | A reconstructed non-filter raw token ends with an unconsumed escape. Matching-prefix Literal routes limit manager-level scope; snippet-local no-tag preservation does not undo upstream reconstruction. |
| User value | Exact remaining token spelling is retained; anticipated reduction in surprising query mismatches. No speed claim. |
| Implementation breadth | Narrow shared tokenizer EOF correction plus directly affected caller tests. |
| Validation burden | Pure tokenizer/filter cases, one inert normal/snapshot provider contract and affected snippet-tag case. No native proof needed for the byte invariant. |

No change retains the inconsistency. Repairing each caller would duplicate policy and leave other callers exposed. A parser rewrite would expand grammar/compatibility scope unnecessarily. The appropriate future correction is in the existing tokenizer owner: preserve raw spelling at EOF while retaining decoded value, filter classification/order, quoting and Literal behavior. If a later current master already fixes it, the future task becomes a source-backed no-op or missing regression proof only.

## Disposition and future proof

**READY_FOR_IMPLEMENTATION_REVIEW**, proposed only. Use the complete canonical [AUD-F001 Goal](../follow-on/AUD-F001-preserve-terminal-query-escape/GOAL.md) and [Kickstart](../follow-on/AUD-F001-preserve-terminal-query-escape/KICKSTART.md), preserving planner origin and the reviewer clarification. This author does not create a duplicate package. The pair remains `PROPOSED_NOT_AUTHORIZED` and has not been executed.

Future cases should distinguish empty text, one pending escape, terminal pair, quoted trailing escape, filter mixed with preserved raw token, normal versus Literal provider and tagged versus untagged snippet. Expected outcomes must compare exact raw/decoded strings and unchanged result policy; future filtered execution must verify nonzero selected tests. Current evidence is static source/caller/test inspection only.

[Q1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/query.rs#L113-L173
[Q2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/query.rs#L230-L263
[Q3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1331-L1437
[Q4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L535-L590
[Q5]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/snippets.rs#L407-L445
[Q6]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/qr.rs#L6-L68
[Q7]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/query.rs#L417-L424
[Q8]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1693-L1709
