# Label amendment 001 — reserved `data` head

Status: FROZEN_LABEL_AMENDMENT_V2, before the sole corrective measurement invocation. Original `g02-eval-v1` inputs, manifest, prototype files and initial results remain unchanged. This overlay changes labels, not queries, gates, catalog, scoring, thresholds, candidates or split membership. It is not a replacement v1 freeze.

The initial three policy runs all returned `ProtectedSyntax` with no rows for Q122, `data viewer`. Its original label incorrectly expected natural discovery and a data-tool chooser. The already-frozen [route fixtures](../../route_fixtures.json) reserve `data`; [DataPlugin's source declaration and parser](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/data.rs#L25-L68) establish that it is an explicit head. An unsupported subcommand remains canonical under the declared admission policy. This is a common label-integrity error, not a ranker failure or evidence that current Rust routing was executed. Parent independently corroborated the source and agreed to the same-label amendment across all three policies and both scopes.

Only Q122's expected admission/route/behavior/relevance/forbidden list and label rationale/confidence/provenance/source references change. Its query/context/ID/family/group/split and all other 299 records remain identical. [Before/after record](before-after-Q122.json), [complete v2 labels](queries-v2.jsonl), [counts](counts-v2.json) and [amendment manifest](amendment_manifest.json) preserve the change. Case `source_refs` retain their original base, Goal02 `evaluation/`, rather than this nested directory. The existing protocol's formulas, metric definitions and exclusion rules remain authoritative; the following denominator overlay applies to v2 only.

| Count | Original v1 | Amended v2 |
|---|---:|---:|
| All cases; development/validation/holdout | 300; 173/61/66 | unchanged |
| Conditional positives; development/validation/holdout | 120; 68/34/18 | 119; 68/33/18 |
| Expected ADMIT / gated | 137 / 163 | 136 / 164 |
| Expected ProtectedSyntax | 103 | 104 |
| Forbidden-case / desired-abstention denominator | 180 / 180 | 181 / 181 |
| Expected chooser denominator | 31 | 30 |
| Initial subset absent-any / absent-all positive targets | 38/120; 23/120 | 38/119; 23/119 |

All other gate counts and family/split membership counts stay unchanged. There are now 17 admitted negative fixtures, as before. Full15 and initial11 retain exactly the same amended labels and denominators; no absent-target case is dropped. Frozen v1 summaries remain available and must be labeled v1 when cited.

The sole permitted corrective cycle is a **measurement-only** invocation over all 1,800 saved predictions (three policies × two scopes × 300). A separate script verifies both freezes, verifies unchanged original prototype identities, preserves exact original file bytes in a base64 source receipt, reuses each original result and replay checks, and recomputes observations/aggregates with v2 labels. It must not call a ranker or regenerate predictions. Every result including gate/order/scores/decomposition/full Action+args is compared to its saved original. A canonical JSON result hash provides a portable unchanged-prediction receipt, separate from UTF-8/LF text input hashes.

This consumes the one corrective cycle, with one invocation ≤120 seconds inside the 600-second aggregate budget. No corrective smoke is needed because scoring/admission implementation and predictions are unchanged. No further evaluation or correction cycle is authorized by this amendment. A remaining defect becomes an explicit limitation. Choosing a candidate and final production recommendations belong to M2-B/M3-A, not this correction.

Hash semantics: amendment files use the same strict `utf8-lf-text-v1` canonical bytes as v1 (CRLF→LF only, no BOM/lone CR, no JSON/Unicode reformatting). Manifest excludes itself. It records its parent manifest identity and the complete inherited v1 path/hash mapping, then hashes the four new overlay artifacts. Saved prediction identity uses sorted-key compact JSON UTF-8, explicitly named `canonical-json-result-v1`; source byte receipts use raw bytes and base64. These are distinct identities and are not claims of cross-platform raw-byte equality.
